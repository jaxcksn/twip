//! Shading

use std::rc::Rc;

use crate::docx::props::{Hatch, Shd};
use crate::drawing::colour::Rgba;
use crate::drawing::geom::{Geom, GeomPath, PathFill, Seg};
use crate::drawing::paint::{Paint, ResolvedEffects, ResolvedLine};

use super::fragment::{Item, ShapeItem};

/// Shading as drawn: a flat fill, and a stripe or cross over it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Shade {
    pub fill: Option<u32>,
    pub hatch: Option<Hatch>,
}

impl Shade {
    pub fn of(shd: Option<&Shd>) -> Shade {
        shd.map_or_else(Shade::default, |s| Shade { fill: s.flat(), hatch: s.hatch() })
    }

    pub fn is_none(&self) -> bool {
        self.fill.is_none() && self.hatch.is_none()
    }

    /// The shading over `rect`: flat, a rect; hatched, a shape whose tiles start at `frame`'s top left.
    pub fn draw(self, out: &mut Vec<Item>, frame: [f64; 4], rect: [f64; 4]) {
        if !(rect[2] > rect[0] && rect[3] > rect[1]) {
            return;
        }
        match (self.hatch, self.fill) {
            (Some(h), fill) => out.push(Item::Shape(hatched(frame, rect, fill, h))),
            (None, Some(fill)) => out.push(Item::fill(rect, fill)),
            (None, None) => {}
        }
    }

    /// The shading over `rect` less `holes` (inside it, side by side in one band down): flat, aliased rects around
    /// them; hatched, one shape whose tiles start at `rect`'s top left, the holes cut out.
    pub fn draw_around(self, out: &mut Vec<Item>, rect: [f64; 4], holes: &[[f64; 4]]) {
        let [x0, y0, x1, y1] = rect;
        if self.is_none() || !(x1 > x0 && y1 > y0) {
            return;
        }
        let Some(h) = self.hatch else {
            let fill = self.fill.unwrap_or(0);
            out.extend(around(rect, holes).into_iter().filter(|r| r[2] > r[0] && r[3] > r[1]).map(|r| Item::fill(r, fill)));
            return;
        };
        let mut segs = Vec::with_capacity(5 * (holes.len() + 1));
        segs.extend([Seg::Move([0.0, 0.0]), Seg::Line([x1 - x0, 0.0]), Seg::Line([x1 - x0, y1 - y0]), Seg::Line([0.0, y1 - y0]), Seg::Close]);
        // Each hole wound the other way.
        for h in holes.iter().filter(|h| h[2] > h[0] && h[3] > h[1]) {
            let [a, b, c, d] = [h[0] - x0, h[1] - y0, h[2] - x0, h[3] - y0];
            segs.extend([Seg::Move([a, b]), Seg::Line([a, d]), Seg::Line([c, d]), Seg::Line([c, b]), Seg::Close]);
        }
        out.push(Item::Shape(fill_shape(rect, segs, hatch_paint(self.fill, h))));
    }
}

/// `rect` less `boxes` (side by side, one band down), as rects: above and below the band, and between the boxes.
fn around(rect: [f64; 4], boxes: &[[f64; 4]]) -> Vec<[f64; 4]> {
    let [x0, y0, x1, y1] = rect;
    let (b0, b1) = boxes.iter().fold((y1, y0), |(a, b), r| (a.min(r[1]), b.max(r[3])));
    if b0 >= b1 {
        return vec![rect];
    }
    let mut spans: Vec<(f64, f64)> = boxes.iter().map(|r| (r[0], r[2])).collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out = vec![[x0, y0, x1, b0], [x0, b1, x1, y1]];
    let mut at = x0;
    for (a, b) in spans {
        if a > at {
            out.push([at, b0, a.min(x1), b1]);
        }
        at = at.max(b);
    }
    if at < x1 {
        out.push([at, b0, x1, b1]);
    }
    out
}

fn rgba(c: u32) -> Rgba {
    Rgba { r: (c >> 16) as u8, g: (c >> 8) as u8, b: c as u8, a: 255 }
}

fn hatch_paint(fill: Option<u32>, h: Hatch) -> Paint {
    Paint::Hatch { stripe: h.stripe, fg: rgba(h.colour), bg: fill.map_or(Rgba { r: 0, g: 0, b: 0, a: 0 }, rgba) }
}

/// A stripe or cross over `fill` (none: clear) filling `rect`, a shape the size of `frame` its tiles start from.
pub fn hatched(frame: [f64; 4], rect: [f64; 4], fill: Option<u32>, h: Hatch) -> Rc<ShapeItem> {
    let [fx, fy, ..] = frame;
    let [x0, y0, x1, y1] = [rect[0] - fx, rect[1] - fy, rect[2] - fx, rect[3] - fy];
    let segs = vec![Seg::Move([x0, y0]), Seg::Line([x1, y0]), Seg::Line([x1, y1]), Seg::Line([x0, y1]), Seg::Close];
    fill_shape(frame, segs, hatch_paint(fill, h))
}

/// A decoration filling `segs` (from `frame`'s top left) with `paint`, its colours ink.
fn fill_shape(frame: [f64; 4], segs: Vec<Seg>, paint: Paint) -> Rc<ShapeItem> {
    let geom = Geom {
        paths: vec![GeomPath { segs, fill: PathFill::Norm, stroke: false }],
        text_rect: [0.0, 0.0, frame[2] - frame[0], frame[3] - frame[1]],
        handles: vec![],
        cxn: vec![],
    };
    Rc::new(ShapeItem {
        rect: frame,
        rot: 0.0,
        flip_h: false,
        flip_v: false,
        geom: Rc::new(geom),
        fill: paint,
        line: ResolvedLine::none(),
        effects: ResolvedEffects::default(),
        behind: false,
        float: 0,
        path: Vec::new(),
        picture: false,
        ink: true,
        decoration: true,
        snap: true,
        clip_bottom: None,
        text_float: 0,
        text: None,
        text_path: None,
        of: Default::default(),
    })
}
