//! Vertical text: which way a cell's, frame's or section's lines run, and their lines turned into place.

use std::rc::Rc;

use super::fragment::{Child, GlyphRun, Item, Turned};
use super::job::{Memos, Yield};
use super::story::{CellEnds, StoryEnv, StoryLayout, StoryUnit};

/// The way a vertical flow's lines run and stack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    /// Text runs down, lines stack right to left.
    Down,
    /// Text runs down, lines stack left to right.
    DownEast,
    /// Text runs up, lines stack left to right.
    Up,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Vertical {
    pub flow: Flow,
    /// East Asian glyphs stay upright.
    pub upright: bool,
}

const fn v(flow: Flow, upright: bool) -> Option<Vertical> {
    Some(Vertical { flow, upright })
}

/// A table cell's or frame's text-flow code (`crate::docx::text_flow`) as its flow; `None` is horizontal.
pub fn of_cell(code: u8) -> Option<Vertical> {
    match code {
        1 => v(Flow::Down, false),
        3 => v(Flow::Up, false),
        5 => v(Flow::Down, true),
        7 => v(Flow::DownEast, true),
        _ => None,
    }
}

/// A section's text-flow code as its flow: a section's `btLr` runs down like `tbRl`.
pub fn of_section(code: u8) -> Option<Vertical> {
    match code {
        1 => v(Flow::Down, true),
        3 => v(Flow::Down, false),
        5 => v(Flow::DownEast, true),
        _ => None,
    }
}

/// The extra ascent and descent of a run in a vertical line: an em/64 each side.
pub fn extra(size: f64) -> f64 {
    super::units::Lu(((size * 2.0).round() * 32.0) as i32).to_pt()
}

/// `blocks` laid out in flow `v` with lines `len` long.
pub fn story(
    blocks: &[&crate::docx::Block],
    len: f64,
    v: Vertical,
    env: &StoryEnv,
    cell: Option<CellEnds>,
    memos: &mut Memos,
) -> Result<std::rc::Rc<StoryLayout>, Yield> {
    let text = super::env::TextEnv { vertical: Some(v), ..env.ctx.env.text.clone() };
    let le = super::LayoutEnv { text, ..env.ctx.env.clone() };
    let venv = StoryEnv { ctx: super::LayoutCtx { env: &le, ..env.ctx }, ..*env };
    super::story::layout(blocks, len.max(1.0), &venv, cell, memos)
}

/// `units`, laid out with their lines along x and stacked down y, turned into `rect` = `[x0, y0, x1, y1]`: each
/// line starts at the flow's start edge (top, or bottom for `Up`), the first line against the flow's first side,
/// `across` further in. Glyphs whose ink lies `bottom` or more across from the first side are not drawn.
pub fn turned(units: &[StoryUnit], flow: Flow, rect: [f64; 4], across: f64, bottom: f64) -> Vec<Item> {
    let [x0, y0, x1, y1] = rect;
    let unit_items = |u: &StoryUnit| {
        let top = u.top + across;
        let children = super::table::clip_x(&u.content, f64::NEG_INFINITY, f64::INFINITY, bottom - top).unwrap_or_else(|| u.content.clone());
        super::traverse::items_of(&[Child::Group { offset: (0.0, top), children }])
    };
    let turn = |centre: [f64; 2], deg: f64, items: Vec<Item>| Item::Turned(Rc::new(Turned { centre, deg, items }));
    match flow {
        Flow::Down => vec![turn([(x1 - y0) / 2.0, (x1 + y0) / 2.0], 90.0, units.iter().flat_map(unit_items).collect())],
        Flow::Up => vec![turn([(x0 + y1) / 2.0, (y1 - x0) / 2.0], 270.0, units.iter().flat_map(unit_items).collect())],
        // Each line turned on its own, its top towards the next line.
        Flow::DownEast => units
            .iter()
            .map(|u| {
                let s = x0 + 2.0 * (u.top + across) + u.height;
                turn([(s - y0) / 2.0, (s + y0) / 2.0], 90.0, unit_items(u))
            })
            .collect(),
    }
}

/// The glyph runs in a turned item, each with its origin on the page and its direction (rounded to a unit step for
/// quarter turns).
pub fn runs(t: &Turned) -> Vec<(&GlyphRun, [f64; 2], [f64; 2])> {
    // m maps the item's coordinates to the page's: x' = m0 x + m2 y + m4, y' = m1 x + m3 y + m5.
    fn walk<'a>(t: &'a Turned, m: [f64; 6], out: &mut Vec<(&'a GlyphRun, [f64; 2], [f64; 2])>) {
        let (sin, cos) = t.deg.to_radians().sin_cos();
        let [cx, cy] = t.centre;
        let r = [cos, sin, -sin, cos, cx - cx * cos + cy * sin, cy - cx * sin - cy * cos];
        let m = [
            m[0] * r[0] + m[2] * r[1],
            m[1] * r[0] + m[3] * r[1],
            m[0] * r[2] + m[2] * r[3],
            m[1] * r[2] + m[3] * r[3],
            m[0] * r[4] + m[2] * r[5] + m[4],
            m[1] * r[4] + m[3] * r[5] + m[5],
        ];
        for i in &t.items {
            match i {
                Item::Glyphs(g) => {
                    let at = [m[0] * g.x + m[2] * g.baseline + m[4], m[1] * g.x + m[3] * g.baseline + m[5]];
                    out.push((g, at, [m[0].round() + 0.0, m[1].round() + 0.0]));
                }
                Item::Turned(inner) => walk(inner, m, out),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(t, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], &mut out);
    out
}
