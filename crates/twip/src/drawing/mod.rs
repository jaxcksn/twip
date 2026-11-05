//! DrawingML objects as a document holds them: shapes, pictures, groups and the graphics the engine keeps without
//! drawing, with their geometry, paint and text.

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

/// A shape's transform as written (`a:xfrm`): offset and extent (EMU), rotation and flips. A group's child frame is in
/// [`GroupShape`].
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Xfrm {
    pub off: [Emu; 2],
    pub ext: [Emu; 2],
    pub rot: Angle,
    pub flip_h: bool,
    pub flip_v: bool,
}

/// `wp:docPr` / `cNvPr` and the locks that matter for editing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NonVisual {
    pub id: u32,
    pub name: String,
    /// `descr`, else `title`: the alternative text.
    pub descr: Rc<str>,
    pub hidden: bool,
    pub lock_aspect: bool,
    /// Marked decorative (`adec:decorative` in the extension list): screen readers skip it.
    pub decorative: bool,
}

/// Byte ranges in the part's XML of the shape element and of the properties it parses, for saving only what changed.
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

/// The text of a shape.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum ShapeText {
    #[default]
    None,
    /// `wps:txbx`: the shape has its own story (the float or group part holds it).
    Story,
    /// `wps:linkedTxbx`: a later box of the chain whose first box holds `wps:txbx id`.
    Linked { chain: u32, seq: u32 },
}

/// `wps:wsp` (and a VML shape read into it).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpShape {
    pub geom: Geometry,
    /// `None`: not written (the style reference or nothing decides).
    pub fill: Option<Fill>,
    pub line: Option<Line>,
    pub effects: Option<EffectList>,
    pub style: Option<StyleRefs>,
    pub body: BodyProps,
    pub text: ShapeText,
    /// `wps:txbx/@id`: the shape holds the story of the chain of linked boxes with this id.
    pub chain: Option<u32>,
    /// `wps:cNvSpPr/@txBox`.
    pub text_box: bool,
    /// `wps:cNvCnPr`: a connector.
    pub connector: bool,
    /// 3D (`a:scene3d`, `a:sp3d`) or an effect DAG: kept on save, not drawn.
    pub kept_3d: bool,
}

/// `pic:pic` (and `v:image`).
#[derive(Clone, Debug, PartialEq)]
pub struct PicShape {
    pub geom: Geometry,
    pub blip: BlipFill,
    pub line: Option<Line>,
    pub effects: Option<EffectList>,
    pub kept_3d: bool,
}

/// `wpg:wgp`, `wpg:grpSp`, `wpc:wpc`, a SmartArt diagram's drawing.
#[derive(Clone, Debug, PartialEq)]
pub struct GroupShape {
    pub child_off: [Emu; 2],
    pub child_ext: [Emu; 2],
    /// The fill of children with `a:grpFill`.
    pub fill: Option<Fill>,
    pub children: Vec<Shape>,
    pub canvas: bool,
}

/// A graphic the engine keeps and positions without drawing (charts, ink, unknown graphic data).
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

/// A DrawingML object.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    pub nv: NonVisual,
    pub xfrm: Xfrm,
    pub kind: ShapeKind,
    pub spans: Spans,
}

/// The index path of a group child (empty: the shape itself).
pub type ChildPath = Vec<u16>;

/// A drawn shape of a tree (a shape, or a group's leaf child) at its place: its unrotated box in the coordinates the
/// tree was placed in (points), and its rotation and flips composed with its groups'.
#[derive(Clone, Debug)]
pub struct Leaf<'a> {
    pub path: ChildPath,
    pub rect: [f64; 4],
    /// Degrees, clockwise.
    pub rot: f64,
    pub flip_h: bool,
    pub flip_v: bool,
    pub shape: &'a Shape,
    /// The nearest enclosing group's fill, for `a:grpFill`.
    pub group_fill: Option<&'a Fill>,
}

/// What a new shape is made as.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NewShape {
    /// A preset shape.
    Shape(geom::Preset),
    /// A line or connector (`line`, `straightConnector1`, `bentConnector3`, ...).
    Line(geom::Preset),
    TextBox,
}

impl Shape {
    /// A new shape `w` × `h` as Word makes it (research R14, `exp\s67\TPL_word.docx`): a shape takes the theme's
    /// accent 1 through its style references, a line the theme's thin accent 1 line, a text box no fill and a half-point
    /// black outline. `name` names its kind ("Rectangle"): the writer adds the drawing's id.
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

    /// The shape's geometry: a shape's or picture's; a group or raw graphic has none.
    pub fn geometry(&self) -> Option<&Geometry> {
        match &self.kind {
            ShapeKind::Sp(s) => Some(&s.geom),
            ShapeKind::Pic(p) => Some(&p.geom),
            ShapeKind::Group(_) | ShapeKind::Raw(_) => None,
        }
    }

    /// The child at `path` (the shape itself for an empty path).
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

    /// Every shape of the tree in drawing order, depth first, with its path (groups before their children).
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

    /// FLOAT-15, DRAW-4: the drawn shapes of the tree placed at `rect` (the shape's box, points), groups mapped to
    /// their children: per axis `s = ext / chExt` (1 when `chExt` is 0), child box = origin + (off − chOff) × s, size
    /// ext × s; nested groups compose. A group's rotation turns its children's centres about the group's centre and
    /// adds to their rotation; a flip mirrors their centres through it and toggles their flips.
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

/// Where a shape of a tree is drawn: its unrotated box and its rotation and flips composed with its groups'.
pub type Frame = ([f64; 4], f64, bool, bool);

impl Shape {
    /// The frame of the shape at `path` (a group's too) for the tree placed at `rect`, as [`Shape::leaves`] places it.
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
    /// Points per EMU of the children's coordinates, per axis, for the group drawn at `rect`.
    pub fn scale(&self, rect: [f64; 4]) -> [f64; 2] {
        let scale = |len: f64, ext: Emu| if ext.0 > 0 { len / ext.to_pt() } else { 1.0 };
        [scale(rect[2] - rect[0], self.child_ext[0]), scale(rect[3] - rect[1], self.child_ext[1])]
    }

    /// Child `c`'s frame in the group drawn at `rect` with `rot` and flips (composed).
    fn child_frame(&self, c: &Shape, rect: [f64; 4], rot: f64, flip_h: bool, flip_v: bool) -> Frame {
        let [x0, y0, x1, y1] = rect;
        let [sx, sy] = self.scale(rect);
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let (sin, cos) = rot.to_radians().sin_cos();
        let w = c.xfrm.ext[0].to_pt() * sx;
        let h = c.xfrm.ext[1].to_pt() * sy;
        let mut px = x0 + (c.xfrm.off[0] - self.child_off[0]).to_pt() * sx + w / 2.0;
        let mut py = y0 + (c.xfrm.off[1] - self.child_off[1]).to_pt() * sy + h / 2.0;
        // The group's flips mirror the child's centre through the group's centre, then its rotation turns it.
        if flip_h {
            px = 2.0 * cx - px;
        }
        if flip_v {
            py = 2.0 * cy - py;
        }
        let (dx, dy) = (px - cx, py - cy);
        let (px, py) = (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos);
        let own = c.xfrm.rot.normalized().degrees();
        // A mirrored group turns its children the other way.
        let child_rot = if flip_h != flip_v { -own } else { own };
        let r = (rot + child_rot).rem_euclid(360.0);
        ([px - w / 2.0, py - h / 2.0, px + w / 2.0, py + h / 2.0], r, c.xfrm.flip_h != flip_h, c.xfrm.flip_v != flip_v)
    }

    /// Grows the children's frame (and the group's own box, `xfrm`, at the same scale) to hold every child's box.
    /// The growth of the group's box at each side in its parent's coordinates: [left, top, right, bottom] (EMU).
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
            // The group's box keeps its scale (EMU of the box per EMU of the children).
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
        // A 200 x 100 pt group whose child frame is 100 x 100: x scale 2. Inside, a group of scale 1.5 in y.
        let inner = group([50 * PT, 0], [50 * PT, 60 * PT], [0, 0], [50 * PT, 40 * PT], vec![rect_shape([0, 20 * PT], [10 * PT, 20 * PT])]);
        let g = group([0, 0], [200 * PT, 100 * PT], [0, 0], [100 * PT, 100 * PT], vec![rect_shape([10 * PT, 10 * PT], [20 * PT, 30 * PT]), inner]);
        let leaves = g.leaves([72.0, 100.0, 272.0, 200.0]);
        assert_eq!(leaves.len(), 2);
        assert_eq!(leaves[0].rect, [92.0, 110.0, 132.0, 140.0]);
        assert_eq!(leaves[0].path, vec![0]);
        // Inner group box: x 72 + 50·2 = 172, w 100; y 100, h 60; its child: y 100 + 20·1.5 = 130, h 30.
        assert_eq!(leaves[1].rect, [172.0, 130.0, 192.0, 160.0]);
        assert_eq!(leaves[1].path, vec![1, 0]);
    }

    #[test]
    fn a_rotated_flipped_group_turns_and_mirrors_its_children() {
        let mut g = group([0, 0], [100 * PT, 100 * PT], [0, 0], [100 * PT, 100 * PT], vec![rect_shape([0, 0], [20 * PT, 20 * PT])]);
        g.xfrm.rot = Angle(90 * 60000);
        let l = &g.leaves([0.0, 0.0, 100.0, 100.0])[0];
        // Child centre (10, 10) turned 90° clockwise about (50, 50): (90, 10).
        assert!((l.rect[0] - 80.0).abs() < 1e-9 && (l.rect[1] - 0.0).abs() < 1e-9);
        assert_eq!(l.rot, 90.0);
        g.xfrm.flip_h = true;
        let l = &g.leaves([0.0, 0.0, 100.0, 100.0])[0];
        // Mirrored first: centre (90, 10), then turned: (90, 90).
        assert!((l.rect[0] - 80.0).abs() < 1e-9 && (l.rect[1] - 80.0).abs() < 1e-9);
        assert!(l.flip_h);
    }
}
