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

/// The extra ascent and descent of a run in a vertical line: an em/64 each side. An East Asian face's 15% extra
/// grows with the height it is taken of.
pub fn extra(face: Option<&crate::font::Face>, size: f64) -> f64 {
    let e = super::units::Lu(((size * 2.0).round() * 32.0) as i32).to_pt();
    match face.filter(|f| f.east_asian) {
        Some(f) => {
            let w = f.ascent + f.descent - 2.0 * f.ea_extra + 2.0 * (f.upem / 64.0).round();
            e + super::units::Lu::from_font((0.15 * w + 0.5).floor() - f.ea_extra, size, f.upem).to_pt()
        }
        None => e,
    }
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

/// How far below a centred line's reference baseline a run sits (points): its runs are centred on the line, so a
/// run's Roman ascent exceeds its centred one by `A + e − (A + D + 2e) div 2` (font units, `A` and `D` the win
/// metrics; the line gap or an East Asian face's extra is on both and cancels).
pub fn centred_drop(face: &crate::font::Face, size: f64) -> f64 {
    let (a, d) = ((face.ascent - face.ea_extra).round() as i64, (face.descent - face.ea_extra).round() as i64);
    let e = (face.upem / 64.0).round() as i64;
    super::units::Lu::from_font((a + e - (a + d + 2 * e) / 2) as f64, size, face.upem).to_pt()
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
                let s = x0 + 2.0 * (u.top + across) + u.height - u.below;
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

/// Two sections share a page across a continuous break only when their text flows the same way.
pub fn same_page(prev: &crate::docx::Section, next: &crate::docx::Section) -> bool {
    next.break_kind == crate::docx::SectionBreak::Continuous
        && of_section(prev.text_flow).map(|v| v.flow) == of_section(next.text_flow).map(|v| v.flow)
}

/// A vertical section's page body, laid out with its lines along x and stacked down y on the page turned a quarter
/// (`page` = the page's width and height as printed), turned into place. Its structure stays: each line's items,
/// each table's backgrounds and borders are turned where they are.
pub fn turn_body(children: &[Child], flow: Flow, page: [f64; 2]) -> Vec<Child> {
    children.iter().map(|c| turn_child(c, flow, page, 0.0, 0.0)).collect()
}

fn turn_child(c: &Child, flow: Flow, page: [f64; 2], ox: f64, oy: f64) -> Child {
    use super::fragment::{BoxFragment, CellFragment, ParaFragment, RowFragment, TableFragment};
    let kids =
        |children: &[Child], dx: f64, dy: f64| -> Vec<Child> { children.iter().map(|c| turn_child(c, flow, page, ox + dx, oy + dy)).collect() };
    match c {
        Child::Box(b) => Child::Box(BoxFragment { children: kids(&b.children, b.offset.0, b.offset.1).into(), ..b.clone() }),
        Child::Group { offset, children } => Child::Group { offset: *offset, children: kids(children, offset.0, offset.1).into() },
        Child::Para(p) => Child::Para(ParaFragment {
            para: p.para,
            lines: p.lines.iter().map(|l| turn_line(l, flow, page, ox, oy)).collect(),
            line_numbers: p.line_numbers.iter().map(|items| turn_items(items, flow, page, [ox, oy], None)).collect(),
        }),
        Child::Line { para, placed } => Child::Line { para: *para, placed: turn_line(placed, flow, page, ox, oy) },
        Child::Items(items) => Child::Items(turn_items(items, flow, page, [ox, oy], None)),
        Child::Table(t) => Child::Table(TableFragment {
            table: t.table,
            rows: t
                .rows
                .iter()
                .map(|r| RowFragment {
                    background: turn_items(&r.background, flow, page, [ox, oy], None),
                    cells: r.cells.iter().map(|c| CellFragment { content: kids(&c.content, 0.0, 0.0).into(), ..c.clone() }).collect(),
                    borders: turn_items(&r.borders, flow, page, [ox, oy], None),
                    ..r.clone()
                })
                .collect(),
        }),
    }
}

fn turn_line(l: &super::fragment::PlacedLine, flow: Flow, page: [f64; 2], ox: f64, oy: f64) -> super::fragment::PlacedLine {
    let (x, y) = (ox + l.x, oy + l.y);
    let f = &l.line;
    let line = super::fragment::LineFragment {
        items: turn_items(&f.items, flow, page, [x, y], Some((y, f.pitch - f.below))),
        pitch: f.pitch,
        baseline: f.baseline,
        below: f.below,
        atoms: f.atoms.clone(),
        changed: f.changed,
        mark: f.mark,
        texts: f.texts.clone(),
        expansions: f.expansions.clone(),
    };
    super::fragment::PlacedLine { x: l.x, y: l.y, line: Rc::new(line) }
}

/// `items` at `origin` turned: about the page's turning point, or, for lines stacked left to right, each line
/// (`band` = its top and pitch on the turned page) or other item about its own band so that it keeps its place.
fn turn_items(items: &[Item], flow: Flow, page: [f64; 2], origin: [f64; 2], band: Option<(f64, f64)>) -> Box<[Item]> {
    if items.is_empty() {
        return Box::new([]);
    }
    let turn = |centre: [f64; 2], deg: f64, items: Vec<Item>| {
        Item::Turned(Rc::new(Turned { centre: [centre[0] - origin[0], centre[1] - origin[1]], deg, items }))
    };
    let [w, h] = page;
    match (flow, band) {
        (Flow::Down, _) => Box::new([turn([w / 2.0, w / 2.0], 90.0, items.to_vec())]),
        (Flow::Up, _) => Box::new([turn([h / 2.0, h / 2.0], 270.0, items.to_vec())]),
        (Flow::DownEast, Some((top, pitch))) => {
            let s = 2.0 * top + pitch;
            Box::new([turn([s / 2.0, s / 2.0], 90.0, items.to_vec())])
        }
        (Flow::DownEast, None) => items
            .iter()
            .map(|it| {
                let (t, b) = match it {
                    Item::Rect { rect, .. } | Item::Image { rect, .. } => (rect[1], rect[3]),
                    Item::Gridline { line } => (line[1], line[3]),
                    Item::Shape(s) => (s.rect[1], s.rect[3]),
                    Item::Glyphs(r) => (r.baseline - 0.8 * r.size, r.baseline + 0.2 * r.size),
                    Item::Turned(t) => (t.centre[1], t.centre[1]),
                };
                let s = 2.0 * origin[1] + t + b;
                turn([s / 2.0, s / 2.0], 90.0, vec![it.clone()])
            })
            .collect(),
    }
}
