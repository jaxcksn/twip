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

/// How far above a centred run's glyph baseline its middle is (points): the drop plus half its centred ascent
/// less its centred descent.
fn centred_middle(face: &crate::font::Face, size: f64) -> f64 {
    let (a, d) = ((face.ascent - face.ea_extra).round() as i64, (face.descent - face.ea_extra).round() as i64);
    let e = (face.upem / 64.0).round() as i64;
    let h = a + d + 2 * e;
    let gap = if face.east_asian { 0 } else { face.leading.round() as i64 };
    let (up, down) = (h / 2 + gap, h - h / 2);
    centred_drop(face, size) + super::units::Lu::from_font((up - down) as f64 / 2.0, size, face.upem).to_pt()
}

/// `units`, laid out with their lines along x and stacked down y, turned into `rect` = `[x0, y0, x1, y1]`: each
/// line starts at the flow's start edge (top, or bottom for `Up`), the first line against the flow's first side,
/// `across` further in. Glyphs whose ink lies `bottom` or more across from the first side are not drawn.
pub fn turned(units: &[StoryUnit], v: Vertical, rect: [f64; 4], across: f64, bottom: f64) -> Vec<Item> {
    let [x0, y0, x1, y1] = rect;
    let unit_items = |u: &StoryUnit| {
        let top = u.top + across;
        let children = super::table::clip_x(&u.content, f64::NEG_INFINITY, f64::INFINITY, bottom - top).unwrap_or_else(|| u.content.clone());
        upright(&super::traverse::items_of(&[Child::Group { offset: (0.0, top), children }]), v)
    };
    let turn = |centre: [f64; 2], deg: f64, items: Vec<Item>| Item::Turned(Rc::new(Turned { centre, deg, items }));
    match v.flow {
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
pub fn turn_body(children: &[Child], v: Vertical, page: [f64; 2]) -> Vec<Child> {
    let f = |items: &[Item], origin: [f64; 2], band: Option<(f64, f64)>| turn_items(&upright(items, v), v.flow, page, origin, band);
    children.iter().map(|c| map_child(c, &f, 0.0, 0.0)).collect()
}

/// A horizontal flow's East Asian glyphs turned a quarter counter-clockwise (`lrTbV`), each about the middle of its
/// em box.
pub fn turn_east_asian(children: &[Child]) -> Vec<Child> {
    let f = |items: &[Item], _: [f64; 2], _: Option<(f64, f64)>| east_asian_turned(items, false).into();
    children.iter().map(|c| map_child(c, &f, 0.0, 0.0)).collect()
}

/// The item lists `c` holds rewritten by `f(items, where they are, the line's top and text height)`.
type ItemMap<'a> = dyn Fn(&[Item], [f64; 2], Option<(f64, f64)>) -> Box<[Item]> + 'a;

fn map_child(c: &Child, f: &ItemMap, ox: f64, oy: f64) -> Child {
    use super::fragment::{BoxFragment, CellFragment, ParaFragment, RowFragment, TableFragment};
    let kids = |children: &[Child], dx: f64, dy: f64| -> Vec<Child> { children.iter().map(|c| map_child(c, f, ox + dx, oy + dy)).collect() };
    match c {
        Child::Box(b) => Child::Box(BoxFragment { children: kids(&b.children, b.offset.0, b.offset.1).into(), ..b.clone() }),
        Child::Group { offset, children } => Child::Group { offset: *offset, children: kids(children, offset.0, offset.1).into() },
        Child::Para(p) => Child::Para(ParaFragment {
            para: p.para,
            lines: p.lines.iter().map(|l| map_line(l, f, ox, oy)).collect(),
            line_numbers: p.line_numbers.iter().map(|items| f(items, [ox, oy], None)).collect(),
        }),
        Child::Line { para, placed } => Child::Line { para: *para, placed: map_line(placed, f, ox, oy) },
        Child::Items(items) => Child::Items(f(items, [ox, oy], None)),
        Child::Table(t) => Child::Table(TableFragment {
            table: t.table,
            rows: t
                .rows
                .iter()
                .map(|r| RowFragment {
                    background: f(&r.background, [ox, oy], None),
                    cells: r.cells.iter().map(|c| CellFragment { content: kids(&c.content, 0.0, 0.0).into(), ..c.clone() }).collect(),
                    borders: f(&r.borders, [ox, oy], None),
                    ..r.clone()
                })
                .collect(),
        }),
    }
}

fn map_line(l: &super::fragment::PlacedLine, f: &ItemMap, ox: f64, oy: f64) -> super::fragment::PlacedLine {
    let (x, y) = (ox + l.x, oy + l.y);
    let line = &l.line;
    let line = super::fragment::LineFragment {
        items: f(&line.items, [x, y], Some((y, line.pitch - line.below))),
        pitch: line.pitch,
        baseline: line.baseline,
        below: line.below,
        atoms: line.atoms.clone(),
        changed: line.changed,
        mark: line.mark,
        texts: line.texts.clone(),
        expansions: line.expansions.clone(),
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

/// `items` with their East Asian glyphs set upright when the flow keeps them so: each turned back a quarter about the
/// middle of its em box, put on the middle of its centred run.
fn upright(items: &[Item], v: Vertical) -> Vec<Item> {
    if v.upright { east_asian_turned(items, true) } else { items.to_vec() }
}

/// `items` with each East Asian glyph turned a quarter counter-clockwise about the middle of its em box, put on the
/// middle of its run when the run is `centred`, else where the box is.
fn east_asian_turned(items: &[Item], centred: bool) -> Vec<Item> {
    let ea = |r: &GlyphRun, x: f64| r.chars.iter().any(|c| (c.1 - x).abs() < 1e-6 && super::text::is_east_asian(c.0 as u32));
    if !items.iter().any(|i| matches!(i, Item::Glyphs(r) if r.glyphs.iter().any(|g| ea(r, g.1)))) {
        return items.to_vec();
    }
    let mut out = Vec::with_capacity(items.len());
    for it in items {
        let Item::Glyphs(r) = it else {
            out.push(it.clone());
            continue;
        };
        let k = r.face.scale(r.size);
        // The em box's middle above the glyph's baseline, and where it goes.
        let lift = (r.face.ascent - r.face.descent) / 2.0 * k;
        let middle = r.baseline - if centred { centred_middle(&r.face, r.size) } else { lift };
        let mut rest = Vec::new();
        for (i, &(g, x)) in r.glyphs.iter().enumerate() {
            if !ea(r, x) {
                rest.push((g, x));
                continue;
            }
            let advance = r.glyphs.get(i + 1).map_or(r.face.advance(g) * k, |n| n.1 - x);
            let chars = r.chars.iter().copied().filter(|c| (c.1 - x).abs() < 1e-6).collect();
            let glyph = GlyphRun { glyphs: Box::new([(g, x)]), chars, x, baseline: middle + lift, ..r.clone() };
            out.push(Item::Turned(Rc::new(Turned { centre: [x + advance / 2.0, middle], deg: -90.0, items: vec![Item::Glyphs(glyph)] })));
        }
        if !rest.is_empty() {
            let chars = r.chars.iter().copied().filter(|c| !super::text::is_east_asian(c.0 as u32)).collect();
            out.push(Item::Glyphs(GlyphRun { glyphs: rest.into(), chars, ..r.clone() }));
        }
    }
    out
}
