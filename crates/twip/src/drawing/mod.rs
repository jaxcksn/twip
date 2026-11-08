//! DrawingML objects

pub mod colour;
pub mod geom;
pub mod paint;
pub mod patterns;
pub mod read;
pub mod text;
pub mod theme;
pub mod units;
pub mod vml;

use std::ops::Range;
use std::rc::Rc;

use geom::Geometry;
use paint::{BlipFill, EffectList, Fill, Line, StyleRefs};
use text::BodyProps;
use units::{Angle, Emu};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Xfrm {
    pub off: [Emu; 2],
    pub ext: [Emu; 2],
    pub rot: Angle,
    pub flip_h: bool,
    pub flip_v: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NonVisual {
    pub id: u32,
    pub name: String,
    pub descr: Rc<str>,
    pub hidden: bool,
    pub lock_aspect: bool,
    pub decorative: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Spans {
    pub shape: Option<Range<usize>>,
    pub sp_pr: Option<Range<usize>>,
    pub xfrm: Option<Range<usize>>,
    pub geom: Option<Range<usize>>,
    pub fill: Option<Range<usize>>,
    pub line: Option<Range<usize>>,
    pub effects: Option<Range<usize>>,
    pub body: Option<Range<usize>>,
    pub style: Option<Range<usize>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum ShapeText {
    #[default]
    None,
    Story,
    Linked {
        chain: u32,
        seq: u32,
    },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpShape {
    pub geom: Geometry,
    pub fill: Option<Fill>,
    pub line: Option<Line>,
    pub effects: Option<EffectList>,
    pub style: Option<StyleRefs>,
    pub body: BodyProps,
    pub text: ShapeText,
    pub chain: Option<u32>,
    pub text_box: bool,
    pub connector: bool,
    pub kept_3d: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PicShape {
    pub geom: Geometry,
    pub blip: BlipFill,
    pub line: Option<Line>,
    pub effects: Option<EffectList>,
    pub kept_3d: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GroupShape {
    pub child_off: [Emu; 2],
    pub child_ext: [Emu; 2],
    pub fill: Option<Fill>,
    pub children: Vec<Shape>,
    pub canvas: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RawGraphic {
    pub uri: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ShapeKind {
    Sp(Box<SpShape>),
    Pic(Box<PicShape>),
    Group(GroupShape),
    Raw(RawGraphic),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub nv: NonVisual,
    pub xfrm: Xfrm,
    pub kind: ShapeKind,
    pub spans: Spans,
}

pub type ChildPath = Vec<u16>;

#[derive(Clone, Debug)]
pub struct Leaf<'a> {
    pub path: ChildPath,
    pub rect: [f64; 4],
    pub rot: f64,
    pub flip_h: bool,
    pub flip_v: bool,
    pub shape: &'a Shape,
    pub group_fill: Option<&'a Fill>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NewShape {
    Shape(geom::Preset),
    Line(geom::Preset),
    TextBox,
}

impl Shape {
    pub fn new(kind: NewShape, w: Emu, h: Emu, name: &str) -> Shape {
        use colour::{Colour, ColourMod, SchemeSlot};
        use text::VAnchor;
        let accent = || Some(Colour::scheme(SchemeSlot::Accent(1)));
        let preset = |p| Geometry::Preset { preset: p, adj: Vec::new() };
        let sp = match kind {
            NewShape::Shape(p) => SpShape {
                geom: preset(p),
                style: Some(StyleRefs {
                    line: Some((2, Some(Colour::scheme(SchemeSlot::Accent(1)).with(ColourMod::Shade(units::Pct(15000)))))),
                    fill: Some((1, accent())),
                    effect: Some((0, accent())),
                    font: Some(Colour::scheme(SchemeSlot::Lt1)),
                }),
                body: BodyProps { anchor: VAnchor::Centre, ..BodyProps::default() },
                ..SpShape::default()
            },
            NewShape::Line(p) => SpShape {
                geom: preset(p),
                style: Some(StyleRefs {
                    line: Some((1, accent())),
                    fill: Some((0, accent())),
                    effect: Some((0, accent())),
                    font: Some(Colour::scheme(SchemeSlot::Tx1)),
                }),
                connector: true,
                ..SpShape::default()
            },
            NewShape::TextBox => SpShape {
                geom: preset(geom::Preset::Rect),
                fill: Some(Fill::None),
                line: Some(Line { width: Some(Emu(6350)), fill: Some(Fill::Solid(Colour::srgb(0))), ..Line::default() }),
                text: ShapeText::Story,
                text_box: true,
                ..SpShape::default()
            },
        };
        Shape {
            nv: NonVisual { name: name.to_string(), ..NonVisual::default() },
            xfrm: Xfrm { ext: [w, h], ..Xfrm::default() },
            kind: ShapeKind::Sp(Box::new(sp)),
            spans: Spans::default(),
        }
    }

    pub fn sp(&self) -> Option<&SpShape> {
        match &self.kind {
            ShapeKind::Sp(s) => Some(s),
            _ => None,
        }
    }

    pub fn sp_mut(&mut self) -> Option<&mut SpShape> {
        match &mut self.kind {
            ShapeKind::Sp(s) => Some(s),
            _ => None,
        }
    }

    pub fn pic(&self) -> Option<&PicShape> {
        match &self.kind {
            ShapeKind::Pic(p) => Some(p),
            _ => None,
        }
    }

    pub fn group(&self) -> Option<&GroupShape> {
        match &self.kind {
            ShapeKind::Group(g) => Some(g),
            _ => None,
        }
    }

    pub fn geometry(&self) -> Option<&Geometry> {
        match &self.kind {
            ShapeKind::Sp(s) => Some(&s.geom),
            ShapeKind::Pic(p) => Some(&p.geom),
            ShapeKind::Group(_) | ShapeKind::Raw(_) => None,
        }
    }

    pub fn at(&self, path: &[u16]) -> Option<&Shape> {
        match path.split_first() {
            None => Some(self),
            Some((i, rest)) => self.group()?.children.get(*i as usize)?.at(rest),
        }
    }

    pub fn at_mut(&mut self, path: &[u16]) -> Option<&mut Shape> {
        match path.split_first() {
            None => Some(self),
            Some((i, rest)) => match &mut self.kind {
                ShapeKind::Group(g) => g.children.get_mut(*i as usize)?.at_mut(rest),
                _ => None,
            },
        }
    }

    pub fn walk(&self) -> Vec<(ChildPath, &Shape)> {
        fn go<'a>(s: &'a Shape, path: &mut ChildPath, out: &mut Vec<(ChildPath, &'a Shape)>) {
            out.push((path.clone(), s));
            if let Some(g) = s.group() {
                for (i, c) in g.children.iter().enumerate() {
                    path.push(i as u16);
                    go(c, path, out);
                    path.pop();
                }
            }
        }
        let mut out = Vec::new();
        go(self, &mut Vec::new(), &mut out);
        out
    }

    pub fn leaves(&self, rect: [f64; 4]) -> Vec<Leaf<'_>> {
        let mut out = Vec::new();
        let rot = self.xfrm.rot.normalized().degrees();
        self.collect(rect, rot, self.xfrm.flip_h, self.xfrm.flip_v, None, &mut Vec::new(), &mut out);
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn collect<'a>(
        &'a self,
        rect: [f64; 4],
        rot: f64,
        flip_h: bool,
        flip_v: bool,
        group_fill: Option<&'a Fill>,
        path: &mut ChildPath,
        out: &mut Vec<Leaf<'a>>,
    ) {
        let Some(g) = self.group() else {
            out.push(Leaf { path: path.clone(), rect, rot, flip_h, flip_v, shape: self, group_fill });
            return;
        };
        let fill = g.fill.as_ref().or(group_fill);
        for (i, c) in g.children.iter().enumerate() {
            let (r, rot, flip_h, flip_v) = g.child_frame(c, rect, rot, flip_h, flip_v);
            path.push(i as u16);
            c.collect(r, rot, flip_h, flip_v, fill, path, out);
            path.pop();
        }
    }
}

pub type Frame = ([f64; 4], f64, bool, bool);

impl Shape {
    pub fn frame_at(&self, rect: [f64; 4], path: &[u16]) -> Option<Frame> {
        let mut f = (rect, self.xfrm.rot.normalized().degrees(), self.xfrm.flip_h, self.xfrm.flip_v);
        let mut s = self;
        for &i in path {
            let g = s.group()?;
            s = g.children.get(i as usize)?;
            f = g.child_frame(s, f.0, f.1, f.2, f.3);
        }
        Some(f)
    }
}

impl GroupShape {
    pub fn scale(&self, rect: [f64; 4]) -> [f64; 2] {
        let scale = |len: f64, ext: Emu| if ext.0 > 0 { len / ext.to_pt() } else { 1.0 };
        [scale(rect[2] - rect[0], self.child_ext[0]), scale(rect[3] - rect[1], self.child_ext[1])]
    }

    fn child_frame(&self, c: &Shape, rect: [f64; 4], rot: f64, flip_h: bool, flip_v: bool) -> Frame {
        let [x0, y0, x1, y1] = rect;
        let [sx, sy] = self.scale(rect);
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let (sin, cos) = rot.to_radians().sin_cos();
        let w = c.xfrm.ext[0].to_pt() * sx;
        let h = c.xfrm.ext[1].to_pt() * sy;
        let mut px = x0 + (c.xfrm.off[0] - self.child_off[0]).to_pt() * sx + w / 2.0;
        let mut py = y0 + (c.xfrm.off[1] - self.child_off[1]).to_pt() * sy + h / 2.0;
        if flip_h {
            px = 2.0 * cx - px;
        }
        if flip_v {
            py = 2.0 * cy - py;
        }
        let (dx, dy) = (px - cx, py - cy);
        let (px, py) = (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos);
        let own = c.xfrm.rot.normalized().degrees();
        let child_rot = if flip_h != flip_v { -own } else { own };
        let r = (rot + child_rot).rem_euclid(360.0);
        ([px - w / 2.0, py - h / 2.0, px + w / 2.0, py + h / 2.0], r, c.xfrm.flip_h != flip_h, c.xfrm.flip_v != flip_v)
    }

    pub fn fit_children(&mut self, xfrm: &mut Xfrm) -> [i64; 4] {
        let (mut lo, mut hi) = ([i64::MAX; 2], [i64::MIN; 2]);
        for c in &self.children {
            for a in 0..2 {
                lo[a] = lo[a].min(c.xfrm.off[a].0);
                hi[a] = hi[a].max(c.xfrm.off[a].0 + c.xfrm.ext[a].0);
            }
        }
        let mut grow = [0; 4];
        if self.children.is_empty() {
            return grow;
        }
        for a in 0..2 {
            let (o, e) = (self.child_off[a].0, self.child_ext[a].0);
            let (no, ne) = (lo[a].min(o), hi[a].max(o + e));
            if (no, ne) == (o, o + e) {
                continue;
            }
            let k = if e > 0 { xfrm.ext[a].0 as f64 / e as f64 } else { 1.0 };
            let (before, after) = (((o - no) as f64 * k).round() as i64, ((ne - o - e) as f64 * k).round() as i64);
            xfrm.off[a].0 -= before;
            xfrm.ext[a].0 += before + after;
            self.child_off[a].0 = no;
            self.child_ext[a].0 = ne - no;
            (grow[a], grow[a + 2]) = (before, after);
        }
        grow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect_shape(off: [i64; 2], ext: [i64; 2]) -> Shape {
        Shape {
            nv: NonVisual::default(),
            xfrm: Xfrm { off: off.map(Emu), ext: ext.map(Emu), ..Xfrm::default() },
            kind: ShapeKind::Sp(Box::default()),
            spans: Spans::default(),
        }
    }

    fn group(off: [i64; 2], ext: [i64; 2], ch_off: [i64; 2], ch_ext: [i64; 2], children: Vec<Shape>) -> Shape {
        Shape {
            nv: NonVisual::default(),
            xfrm: Xfrm { off: off.map(Emu), ext: ext.map(Emu), ..Xfrm::default() },
            kind: ShapeKind::Group(GroupShape { child_off: ch_off.map(Emu), child_ext: ch_ext.map(Emu), fill: None, children, canvas: false }),
            spans: Spans::default(),
        }
    }

    const PT: i64 = 12700;

    #[test]
    fn groups_map_their_children_and_nested_groups_compose() {
        let inner = group([50 * PT, 0], [50 * PT, 60 * PT], [0, 0], [50 * PT, 40 * PT], vec![rect_shape([0, 20 * PT], [10 * PT, 20 * PT])]);
        let g = group([0, 0], [200 * PT, 100 * PT], [0, 0], [100 * PT, 100 * PT], vec![rect_shape([10 * PT, 10 * PT], [20 * PT, 30 * PT]), inner]);
        let leaves = g.leaves([72.0, 100.0, 272.0, 200.0]);
        assert_eq!(leaves.len(), 2);
        assert_eq!(leaves[0].rect, [92.0, 110.0, 132.0, 140.0]);
        assert_eq!(leaves[0].path, vec![0]);
        assert_eq!(leaves[1].rect, [172.0, 130.0, 192.0, 160.0]);
        assert_eq!(leaves[1].path, vec![1, 0]);
    }

    #[test]
    fn a_rotated_flipped_group_turns_and_mirrors_its_children() {
        let mut g = group([0, 0], [100 * PT, 100 * PT], [0, 0], [100 * PT, 100 * PT], vec![rect_shape([0, 0], [20 * PT, 20 * PT])]);
        g.xfrm.rot = Angle(90 * 60000);
        let l = &g.leaves([0.0, 0.0, 100.0, 100.0])[0];
        assert!((l.rect[0] - 80.0).abs() < 1e-9 && (l.rect[1] - 0.0).abs() < 1e-9);
        assert_eq!(l.rot, 90.0);
        g.xfrm.flip_h = true;
        let l = &g.leaves([0.0, 0.0, 100.0, 100.0])[0];
        assert!((l.rect[0] - 80.0).abs() < 1e-9 && (l.rect[1] - 80.0).abs() < 1e-9);
        assert!(l.flip_h);
    }
}
