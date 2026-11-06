//! SVG pictures (`asvg:svgBlip`): the subset application placeholders and Office icons use. Nested viewports with
//! `viewBox` and `preserveAspectRatio`, percentage lengths, `rect` (with `rx`/`ry`), `circle`, `ellipse`, `line`,
//! `polyline`, `polygon` and `path`, linear and radial gradients (either unit space), clip paths, fill, stroke and
//! opacity, from attributes or `style`. The picture's box is the root viewport, so an SVG with no `viewBox` and
//! percentage geometry lays itself out at whatever size the picture has. Text, `use`, masks, filters, patterns and
//! style sheets aren't drawn.

use std::collections::HashMap;

use roxmltree::{Document, Node};
use skia_safe::gradient::{Colors, Gradient, Interpolation, shaders};
use skia_safe::{AlphaType, Canvas, ClipOp, Color, Color4f, ColorType, ImageInfo, Matrix, Paint, PaintCap, PaintJoin, PaintStyle, Path};
use skia_safe::{PathBuilder, PathFillType, Point, RRect, Rect, TileMode};

/// Draws an SVG into `dst` (the picture's box, which is its viewport); false when the bytes aren't an SVG.
// ponytail: parsed again on every draw; cache the tree per image if large SVGs show up in profiles.
pub fn draw(canvas: &Canvas, bytes: &[u8], dst: Rect) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else { return false };
    let opts = roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
    let Ok(doc) = Document::parse_with_options(text.trim_start_matches('\u{feff}'), opts) else { return false };
    let root = doc.root_element();
    if root.tag_name().name() != "svg" {
        return false;
    }
    let (w, h) = (dst.width(), dst.height());
    if w <= 0.0 || h <= 0.0 {
        return true;
    }
    let svg = Svg { ids: doc.descendants().filter_map(|n| Some((n.attribute("id")?, n))).collect() };
    let vp = Vp { w, h };
    // The root fills the box. With no viewBox, an absolute size makes one, stretched to the box as a picture is.
    let (view_box, align) = match view_box(root) {
        Some(v) => (Some(v), aspect(root)),
        None => {
            let size = |a: &str, axis| root.attribute(a).filter(|v| !v.contains('%')).and_then(|v| length(v, vp, axis));
            (size("width", Axis::X).zip(size("height", Axis::Y)).map(|(a, b)| [0.0, 0.0, a, b]), None)
        }
    };
    canvas.save();
    canvas.translate((dst.left, dst.top));
    canvas.clip_rect(Rect::from_wh(w, h), None, true);
    let inner = fit(canvas, [0.0, 0.0, w, h], view_box, align);
    svg.children(canvas, root, &Style::default(), inner);
    canvas.restore();
    true
}

/// An SVG drawn at `w` × `h` pixels as a PNG: the fallback picture Word writes beside an SVG.
pub fn fallback_png(bytes: &[u8], w: i32, h: i32) -> Option<Vec<u8>> {
    let (w, h) = (w.clamp(1, 2048), h.clamp(1, 2048));
    let mut s = skia_safe::surfaces::raster_n32_premul((w, h))?;
    s.canvas().clear(Color::TRANSPARENT);
    if !draw(s.canvas(), bytes, Rect::from_wh(w as f32, h as f32)) {
        return None;
    }
    let info = ImageInfo::new((w, h), ColorType::RGBA8888, AlphaType::Unpremul, None);
    let mut px = vec![0u8; (w * h * 4) as usize];
    s.read_pixels(&info, &mut px, (w * 4) as usize, (0, 0)).then_some(())?;
    Some(png(w as u32, h as u32, &px))
}

/// RGBA pixels as a PNG.
fn png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut raw = Vec::with_capacity(rgba.len() + h as usize);
    for row in rgba.chunks(w as usize * 4) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    let data = z.write_all(&raw).and_then(|_| z.finish()).unwrap_or_default();
    let chunk = |out: &mut Vec<u8>, kind: &[u8; 4], body: &[u8]| {
        out.extend((body.len() as u32).to_be_bytes());
        let mut crc = flate2::Crc::new();
        crc.update(kind);
        crc.update(body);
        out.extend(kind);
        out.extend(body);
        out.extend(crc.sum().to_be_bytes());
    };
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = [0u8; 13];
    ihdr[..4].copy_from_slice(&w.to_be_bytes());
    ihdr[4..8].copy_from_slice(&h.to_be_bytes());
    ihdr[8..].copy_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &data);
    chunk(&mut out, b"IEND", &[]);
    out
}

#[derive(Clone, Copy, Debug)]
struct Vp {
    w: f32,
    h: f32,
}

#[derive(Clone, Copy)]
enum Axis {
    X,
    Y,
    /// The viewport's normalised diagonal (radii, stroke widths).
    D,
}

#[derive(Clone, Debug)]
enum Fill {
    Colour([f32; 4]),
    /// A gradient by id, and the colour when it isn't one.
    Url(String, Option<[f32; 4]>),
}

#[derive(Clone, Debug)]
struct Style {
    fill: Option<Fill>,
    fill_opacity: f32,
    stroke: Option<Fill>,
    stroke_opacity: f32,
    stroke_width: f32,
    even_odd: bool,
    cap: PaintCap,
    join: PaintJoin,
    miter: f32,
    dash: Option<Vec<f32>>,
    colour: [f32; 4],
    hidden: bool,
}

impl Default for Style {
    fn default() -> Style {
        Style {
            fill: Some(Fill::Colour([0.0, 0.0, 0.0, 1.0])),
            fill_opacity: 1.0,
            stroke: None,
            stroke_opacity: 1.0,
            stroke_width: 1.0,
            even_odd: false,
            cap: PaintCap::Butt,
            join: PaintJoin::Miter,
            miter: 4.0,
            dash: None,
            colour: [0.0, 0.0, 0.0, 1.0],
            hidden: false,
        }
    }
}

/// A property: from `style` first, else the presentation attribute.
fn prop<'a>(n: Node<'a, '_>, name: &str) -> Option<&'a str> {
    let styled = n.attribute("style").and_then(|s| {
        s.split(';').filter_map(|d| d.split_once(':')).find(|(k, _)| k.trim() == name).map(|(_, v)| v.trim().trim_end_matches("!important").trim())
    });
    styled.or_else(|| n.attribute(name)).filter(|v| *v != "inherit")
}

impl Style {
    fn inherit(&self, n: Node, vp: Vp) -> Style {
        let mut s = self.clone();
        if let Some(c) = prop(n, "color").and_then(colour) {
            s.colour = c;
        }
        if let Some(v) = prop(n, "fill").and_then(|v| fill(v, s.colour)) {
            s.fill = v;
        }
        if let Some(v) = prop(n, "stroke").and_then(|v| fill(v, s.colour)) {
            s.stroke = v;
        }
        let unit = |v: Option<&str>| v.and_then(|v| v.trim().parse::<f32>().ok()).map(|v| v.clamp(0.0, 1.0));
        s.fill_opacity = unit(prop(n, "fill-opacity")).unwrap_or(s.fill_opacity);
        s.stroke_opacity = unit(prop(n, "stroke-opacity")).unwrap_or(s.stroke_opacity);
        if let Some(w) = prop(n, "stroke-width").and_then(|v| length(v, vp, Axis::D)) {
            s.stroke_width = w.max(0.0);
        }
        if let Some(r) = prop(n, "fill-rule") {
            s.even_odd = r == "evenodd";
        }
        s.cap = match prop(n, "stroke-linecap") {
            Some("round") => PaintCap::Round,
            Some("square") => PaintCap::Square,
            Some(_) => PaintCap::Butt,
            None => s.cap,
        };
        s.join = match prop(n, "stroke-linejoin") {
            Some("round") => PaintJoin::Round,
            Some("bevel") => PaintJoin::Bevel,
            Some(_) => PaintJoin::Miter,
            None => s.join,
        };
        if let Some(m) = prop(n, "stroke-miterlimit").and_then(|v| v.trim().parse().ok()) {
            s.miter = m;
        }
        match prop(n, "stroke-dasharray") {
            Some("none") => s.dash = None,
            Some(v) => {
                let d: Vec<f32> = numbers(v).into_iter().map(f32::abs).collect();
                s.dash = (!d.is_empty() && d.iter().any(|x| *x > 0.0)).then(|| if d.len() % 2 == 1 { [d.clone(), d].concat() } else { d });
            }
            None => {}
        }
        if let Some(v) = prop(n, "visibility") {
            s.hidden = v == "hidden" || v == "collapse";
        }
        s
    }
}

/// A `fill`/`stroke` value: `Some(None)` for `none`, `None` when it can't be read (the inherited one stays).
fn fill(v: &str, current: [f32; 4]) -> Option<Option<Fill>> {
    let v = v.trim();
    if v == "none" {
        return Some(None);
    }
    if v == "currentColor" {
        return Some(Some(Fill::Colour(current)));
    }
    if let Some(rest) = v.strip_prefix("url(") {
        let (id, after) = rest.split_once(')')?;
        let id = id.trim().trim_matches(['\'', '"']).trim_start_matches('#').to_string();
        let after = after.trim();
        return Some(Some(Fill::Url(id, if after == "currentColor" { Some(current) } else { colour(after) })));
    }
    colour(v).map(|c| Some(Fill::Colour(c)))
}

struct Svg<'a, 'i> {
    ids: HashMap<&'a str, Node<'a, 'i>>,
}

impl<'a, 'i> Svg<'a, 'i> {
    fn children(&self, c: &Canvas, n: Node<'a, 'i>, st: &Style, vp: Vp) {
        for k in n.children().filter(Node::is_element) {
            self.node(c, k, st, vp);
        }
    }

    fn node(&self, c: &Canvas, n: Node<'a, 'i>, parent: &Style, vp: Vp) {
        let name = n.tag_name().name();
        if n.tag_name().namespace().is_some_and(|ns| ns != "http://www.w3.org/2000/svg") || prop(n, "display") == Some("none") {
            return;
        }
        if !matches!(name, "svg" | "g" | "a" | "switch" | "rect" | "circle" | "ellipse" | "line" | "polyline" | "polygon" | "path") {
            return;
        }
        let st = parent.inherit(n, vp);
        let clip = prop(n, "clip-path").and_then(|v| self.url(v));
        let opacity = prop(n, "opacity").and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(1.0).clamp(0.0, 1.0);
        c.save();
        if name != "svg"
            && let Some(m) = n.attribute("transform").and_then(transform)
        {
            c.concat(&m);
        }
        match name {
            "svg" => {
                let l = |a: &str, axis, d: f32| n.attribute(a).and_then(|v| length(v, vp, axis)).unwrap_or(d);
                let r = [l("x", Axis::X, 0.0), l("y", Axis::Y, 0.0), l("width", Axis::X, vp.w), l("height", Axis::Y, vp.h)];
                if r[2] > 0.0 && r[3] > 0.0 {
                    if prop(n, "overflow").is_none_or(|o| o == "hidden" || o == "clip") {
                        c.clip_rect(Rect::from_xywh(r[0], r[1], r[2], r[3]), None, true);
                    }
                    let inner = fit(c, r, view_box(n), aspect(n));
                    self.layer(c, opacity, || self.children(c, n, &st, inner));
                }
            }
            "g" | "a" | "switch" => {
                if let Some(k) = clip {
                    self.clip(c, k, None, vp);
                }
                self.layer(c, opacity, || self.children(c, n, &st, vp));
            }
            _ => {
                if let Some(path) = shape(n, vp).filter(|_| !st.hidden) {
                    if let Some(k) = clip {
                        self.clip(c, k, Some(*path.bounds()), vp);
                    }
                    self.layer(c, opacity, || self.paint(c, path, &st, vp));
                }
            }
        }
        c.restore();
    }

    fn layer(&self, c: &Canvas, opacity: f32, draw: impl FnOnce()) {
        if opacity >= 1.0 {
            return draw();
        }
        c.save_layer_alpha_f(None, opacity);
        draw();
        c.restore();
    }

    fn url(&self, v: &str) -> Option<Node<'a, 'i>> {
        let id = v.trim().strip_prefix("url(")?.split(')').next()?.trim().trim_matches(['\'', '"']).trim_start_matches('#');
        self.ids.get(id).copied()
    }

    /// Clips to a `clipPath`: its shapes, in the user space of what refers to it (or its bounding box's).
    fn clip(&self, c: &Canvas, k: Node, bbox: Option<Rect>, vp: Vp) {
        if k.tag_name().name() != "clipPath" {
            return;
        }
        let mut b = PathBuilder::new();
        for s in k.children().filter(Node::is_element) {
            let Some(mut p) = shape(s, vp) else { continue };
            if let Some(m) = s.attribute("transform").and_then(transform) {
                p = p.make_transform(&m);
            }
            b.add_path(&p, None);
        }
        let mut path = b.detach();
        if prop(k, "clip-rule") == Some("evenodd") {
            path.set_fill_type(PathFillType::EvenOdd);
        }
        let mut m = Matrix::new_identity();
        if k.attribute("clipPathUnits") == Some("objectBoundingBox") {
            let Some(bb) = bbox else { return };
            m = unit_box(bb);
        }
        if let Some(t) = k.attribute("transform").and_then(transform) {
            m.pre_concat(&t);
        }
        c.clip_path(&path.make_transform(&m), ClipOp::Intersect, true);
    }

    fn paint(&self, c: &Canvas, mut path: Path, st: &Style, vp: Vp) {
        path.set_fill_type(if st.even_odd { PathFillType::EvenOdd } else { PathFillType::Winding });
        let bbox = *path.bounds();
        if let Some(mut p) = st.fill.as_ref().and_then(|f| self.paint_of(f, st.fill_opacity, bbox, vp)) {
            p.set_style(PaintStyle::Fill);
            c.draw_path(&path, &p);
        }
        if st.stroke_width > 0.0
            && let Some(mut p) = st.stroke.as_ref().and_then(|f| self.paint_of(f, st.stroke_opacity, bbox, vp))
        {
            p.set_style(PaintStyle::Stroke).set_stroke_width(st.stroke_width).set_stroke_cap(st.cap).set_stroke_join(st.join);
            p.set_stroke_miter(st.miter);
            if let Some(d) = &st.dash {
                p.set_path_effect(skia_safe::PathEffect::dash(d, 0.0));
            }
            c.draw_path(&path, &p);
        }
    }

    fn paint_of(&self, f: &Fill, opacity: f32, bbox: Rect, vp: Vp) -> Option<Paint> {
        let mut p = Paint::default();
        p.set_anti_alias(true);
        let solid = |p: &mut Paint, [r, g, b, a]: [f32; 4]| {
            p.set_color4f(Color4f::new(r, g, b, a * opacity), None);
        };
        match f {
            Fill::Colour(col) => solid(&mut p, *col),
            Fill::Url(id, fallback) => match self.ids.get(id.as_str()) {
                Some(g) if self.gradient(&mut p, *g, bbox, vp) => {
                    p.set_alpha_f(opacity);
                }
                _ => solid(&mut p, (*fallback)?),
            },
        }
        Some(p)
    }

    /// A linear or radial gradient's shader on `p`, its attributes and stops following `href`s.
    fn gradient(&self, p: &mut Paint, g: Node<'a, 'i>, bbox: Rect, vp: Vp) -> bool {
        let mut chain = vec![g];
        while let Some(next) = chain.last().and_then(|n| n.attribute("href").or_else(|| n.attribute(("http://www.w3.org/1999/xlink", "href"))))
            && chain.len() < 8
        {
            match self.ids.get(next.trim_start_matches('#')) {
                Some(n) if !chain.contains(n) => chain.push(*n),
                _ => break,
            }
        }
        let attr = |name: &str| chain.iter().find_map(|n| n.attribute(name));
        let Some(holder) = chain.iter().find(|n| n.children().any(|k| k.tag_name().name() == "stop")) else { return false };
        let mut pos: Vec<f32> = Vec::new();
        let mut cols: Vec<Color4f> = Vec::new();
        for s in holder.children().filter(|k| k.tag_name().name() == "stop") {
            let at = fraction(s.attribute("offset").unwrap_or("0")).unwrap_or(0.0).clamp(0.0, 1.0).max(pos.last().copied().unwrap_or(0.0));
            let [r, g, b, a] = prop(s, "stop-color").and_then(colour).unwrap_or([0.0, 0.0, 0.0, 1.0]);
            let alpha = prop(s, "stop-opacity").and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(1.0).clamp(0.0, 1.0);
            pos.push(at);
            cols.push(Color4f::new(r, g, b, a * alpha));
        }
        if cols.len() == 1 {
            pos.push(1.0);
            cols.push(cols[0]);
        }
        let mode = match attr("spreadMethod") {
            Some("reflect") => TileMode::Mirror,
            Some("repeat") => TileMode::Repeat,
            _ => TileMode::Clamp,
        };
        let bounding = attr("gradientUnits") != Some("userSpaceOnUse");
        if bounding && (bbox.width() <= 0.0 || bbox.height() <= 0.0) {
            return false;
        }
        let coord = |name: &str, default: &str, axis| {
            let v = attr(name).unwrap_or(default);
            if bounding { fraction(v) } else { length(v, vp, axis) }.unwrap_or(0.0)
        };
        let mut m = if bounding { unit_box(bbox) } else { Matrix::new_identity() };
        if let Some(t) = attr("gradientTransform").and_then(transform) {
            m.pre_concat(&t);
        }
        let grad = Gradient::new(Colors::new(&cols, Some(pos.as_slice()), mode, None), Interpolation::default());
        if g.tag_name().name() == "linearGradient" {
            let (a, b) = (
                Point::new(coord("x1", "0%", Axis::X), coord("y1", "0%", Axis::Y)),
                Point::new(coord("x2", "100%", Axis::X), coord("y2", "0%", Axis::Y)),
            );
            p.set_shader(shaders::linear_gradient((a, b), &grad, Some(&m)));
        } else {
            let centre = Point::new(coord("cx", "50%", Axis::X), coord("cy", "50%", Axis::Y));
            p.set_shader(shaders::radial_gradient((centre, coord("r", "50%", Axis::D)), &grad, Some(&m)));
        }
        true
    }
}

/// The matrix mapping the unit square onto a bounding box.
fn unit_box(b: Rect) -> Matrix {
    let mut m = Matrix::translate((b.left, b.top));
    m.pre_scale((b.width(), b.height()), None);
    m
}

/// A basic shape's outline in its user space.
fn shape(n: Node, vp: Vp) -> Option<Path> {
    let l = |a: &str, axis, d: f32| n.attribute(a).and_then(|v| length(v, vp, axis)).unwrap_or(d);
    let mut b = PathBuilder::new();
    match n.tag_name().name() {
        "rect" => {
            let (x, y, w, h) = (l("x", Axis::X, 0.0), l("y", Axis::Y, 0.0), l("width", Axis::X, 0.0), l("height", Axis::Y, 0.0));
            if w <= 0.0 || h <= 0.0 {
                return None;
            }
            let (rx, ry) = (n.attribute("rx").and_then(|v| length(v, vp, Axis::X)), n.attribute("ry").and_then(|v| length(v, vp, Axis::Y)));
            let (rx, ry) = match (rx, ry) {
                (Some(a), Some(b)) => (a, b),
                (Some(a), None) | (None, Some(a)) => (a, a),
                (None, None) => (0.0, 0.0),
            };
            let r = Rect::from_xywh(x, y, w, h);
            b.add_rrect(RRect::new_rect_xy(r, rx.clamp(0.0, w / 2.0), ry.clamp(0.0, h / 2.0)), None, None);
        }
        "circle" => {
            let r = l("r", Axis::D, 0.0);
            if r <= 0.0 {
                return None;
            }
            let (cx, cy) = (l("cx", Axis::X, 0.0), l("cy", Axis::Y, 0.0));
            b.add_oval(Rect::from_ltrb(cx - r, cy - r, cx + r, cy + r), None, None);
        }
        "ellipse" => {
            let (rx, ry) = (l("rx", Axis::X, 0.0), l("ry", Axis::Y, 0.0));
            if rx <= 0.0 || ry <= 0.0 {
                return None;
            }
            let (cx, cy) = (l("cx", Axis::X, 0.0), l("cy", Axis::Y, 0.0));
            b.add_oval(Rect::from_ltrb(cx - rx, cy - ry, cx + rx, cy + ry), None, None);
        }
        "line" => {
            b.move_to((l("x1", Axis::X, 0.0), l("y1", Axis::Y, 0.0))).line_to((l("x2", Axis::X, 0.0), l("y2", Axis::Y, 0.0)));
        }
        k @ ("polyline" | "polygon") => {
            let v = numbers(n.attribute("points")?);
            let pts: Vec<Point> = v.as_chunks::<2>().0.iter().map(|[x, y]| Point::new(*x, *y)).collect();
            if pts.len() < 2 {
                return None;
            }
            b.add_polygon(&pts, k == "polygon");
        }
        "path" => return Path::from_svg(n.attribute("d")?),
        _ => return None,
    }
    Some(b.detach())
}

/// `viewBox`, when it has a positive size.
fn view_box(n: Node) -> Option<[f32; 4]> {
    let v = numbers(n.attribute("viewBox")?);
    (v.len() == 4 && v[2] > 0.0 && v[3] > 0.0).then(|| [v[0], v[1], v[2], v[3]])
}

/// `preserveAspectRatio`: the alignment (x, y as 0, ½ or 1) and slice, or `None` to stretch. Default `xMidYMid meet`.
fn aspect(n: Node) -> Option<(f32, f32, bool)> {
    let v = n.attribute("preserveAspectRatio").unwrap_or("xMidYMid meet");
    let mut parts = v.split_whitespace().filter(|t| *t != "defer");
    let align = parts.next().unwrap_or("xMidYMid");
    if align == "none" {
        return None;
    }
    let at = |s: &str| match s {
        "Min" => 0.0,
        "Max" => 1.0,
        _ => 0.5,
    };
    let (x, y) = (align.get(1..4).map_or(0.5, at), align.get(5..8).map_or(0.5, at));
    Some((x, y, parts.next() == Some("slice")))
}

/// Maps a viewport (`r`: x, y, width, height) to its view box on the canvas; the viewport its children see.
fn fit(c: &Canvas, r: [f32; 4], view_box: Option<[f32; 4]>, align: Option<(f32, f32, bool)>) -> Vp {
    let [x, y, w, h] = r;
    c.translate((x, y));
    let Some([vx, vy, vw, vh]) = view_box else { return Vp { w, h } };
    let (mut sx, mut sy) = (w / vw, h / vh);
    let (mut tx, mut ty) = (0.0, 0.0);
    if let Some((ax, ay, slice)) = align {
        let s = if slice { sx.max(sy) } else { sx.min(sy) };
        (sx, sy) = (s, s);
        (tx, ty) = ((w - vw * s) * ax, (h - vh * s) * ay);
    }
    c.translate((tx, ty));
    c.scale((sx, sy));
    c.translate((-vx, -vy));
    Vp { w: vw, h: vh }
}

/// A length in user units; percentages of the viewport's width, height or normalised diagonal.
fn length(v: &str, vp: Vp, axis: Axis) -> Option<f32> {
    let v = v.trim();
    let k = number_end(v);
    let n: f32 = v[..k].parse().ok()?;
    Some(match v[k..].trim() {
        "" | "px" => n,
        "%" => {
            n / 100.0
                * match axis {
                    Axis::X => vp.w,
                    Axis::Y => vp.h,
                    Axis::D => ((vp.w * vp.w + vp.h * vp.h) / 2.0).sqrt(),
                }
        }
        "pt" => n * 4.0 / 3.0,
        "pc" => n * 16.0,
        "in" => n * 96.0,
        "cm" => n * 96.0 / 2.54,
        "mm" => n * 9.6 / 2.54,
        "em" => n * 16.0,
        "ex" => n * 8.0,
        _ => return None,
    })
}

/// A fraction: `50%` or `0.5`.
fn fraction(v: &str) -> Option<f32> {
    let v = v.trim();
    match v.strip_suffix('%') {
        Some(p) => p.trim().parse::<f32>().ok().map(|p| p / 100.0),
        None => v.parse().ok(),
    }
}

/// Where the number at the start of `s` ends (sign, digits, one point, exponent).
fn number_end(s: &str) -> usize {
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let mut point = false;
    while i < b.len() && (b[i].is_ascii_digit() || (b[i] == b'.' && !point)) {
        point |= b[i] == b'.';
        i += 1;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        if j < b.len() && b[j].is_ascii_digit() {
            i = j;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
        }
    }
    i
}

/// The numbers in a list (`viewBox`, `points`, transform arguments): separated by spaces, commas or signs.
fn numbers(s: &str) -> Vec<f32> {
    let mut out = Vec::new();
    let mut rest = s;
    loop {
        rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == ',');
        if rest.is_empty() {
            return out;
        }
        let k = number_end(rest);
        match rest[..k].parse() {
            Ok(n) if k > 0 => out.push(n),
            _ => return out,
        }
        rest = &rest[k..];
    }
}

/// A `transform` list as one matrix.
fn transform(s: &str) -> Option<Matrix> {
    let mut m = Matrix::new_identity();
    let mut rest = s.trim();
    while !rest.is_empty() {
        let (name, after) = rest.split_once('(')?;
        let (args, after) = after.split_once(')')?;
        let a = numbers(args);
        let arg = |i: usize, d: f32| a.get(i).copied().unwrap_or(d);
        let t = match name.trim().trim_start_matches(',').trim() {
            "matrix" if a.len() == 6 => Matrix::new_all(a[0], a[2], a[4], a[1], a[3], a[5], 0.0, 0.0, 1.0),
            "translate" => Matrix::translate((arg(0, 0.0), arg(1, 0.0))),
            "scale" => Matrix::scale((arg(0, 1.0), arg(1, arg(0, 1.0)))),
            "rotate" => Matrix::rotate_deg_pivot(arg(0, 0.0), (arg(1, 0.0), arg(2, 0.0))),
            "skewX" => Matrix::skew((arg(0, 0.0).to_radians().tan(), 0.0)),
            "skewY" => Matrix::skew((0.0, arg(0, 0.0).to_radians().tan())),
            _ => return None,
        };
        m.pre_concat(&t);
        rest = after.trim_start_matches(|c: char| c.is_whitespace() || c == ',');
    }
    Some(m)
}

/// A CSS colour (hex, `rgb()`/`rgba()` or a basic name) as RGBA fractions.
fn colour(v: &str) -> Option<[f32; 4]> {
    let v = v.trim();
    let rgb = |r: u8, g: u8, b: u8| Some([f32::from(r) / 255.0, f32::from(g) / 255.0, f32::from(b) / 255.0, 1.0]);
    if let Some(h) = v.strip_prefix('#') {
        let d: Vec<u8> = h.chars().map(|c| c.to_digit(16).map(|d| d as u8)).collect::<Option<_>>()?;
        return match d.len() {
            3 | 4 => {
                let mut c = [d[0] * 17, d[1] * 17, d[2] * 17, d.get(3).map_or(255, |a| a * 17)].map(|x| f32::from(x) / 255.0);
                c[3] = c[3].min(1.0);
                Some(c)
            }
            6 | 8 => {
                let byte = |i: usize| d.get(i).map_or(15, |x| *x) * 16 + d.get(i + 1).map_or(15, |x| *x);
                Some([byte(0), byte(2), byte(4), if d.len() == 8 { byte(6) } else { 255 }].map(|x| f32::from(x) / 255.0))
            }
            _ => None,
        };
    }
    if let Some(args) = v.strip_prefix("rgba(").or_else(|| v.strip_prefix("rgb(")) {
        let parts: Vec<&str> = args.trim_end_matches(')').split([',', ' ', '/']).filter(|p| !p.trim().is_empty()).collect();
        let channel = |p: &str| match p.trim().strip_suffix('%') {
            Some(q) => q.parse::<f32>().ok().map(|q| q / 100.0),
            None => p.trim().parse::<f32>().ok().map(|q| q / 255.0),
        };
        let alpha = parts.get(3).and_then(|p| fraction(p)).unwrap_or(1.0);
        return Some([channel(parts.first()?)?, channel(parts.get(1)?)?, channel(parts.get(2)?)?, alpha].map(|x| x.clamp(0.0, 1.0)));
    }
    match v.to_ascii_lowercase().as_str() {
        "black" => rgb(0, 0, 0),
        "white" => rgb(255, 255, 255),
        "red" => rgb(255, 0, 0),
        "green" => rgb(0, 128, 0),
        "lime" => rgb(0, 255, 0),
        "blue" => rgb(0, 0, 255),
        "yellow" => rgb(255, 255, 0),
        "orange" => rgb(255, 165, 0),
        "purple" => rgb(128, 0, 128),
        "fuchsia" | "magenta" => rgb(255, 0, 255),
        "aqua" | "cyan" => rgb(0, 255, 255),
        "teal" => rgb(0, 128, 128),
        "navy" => rgb(0, 0, 128),
        "maroon" => rgb(128, 0, 0),
        "olive" => rgb(128, 128, 0),
        "gray" | "grey" => rgb(128, 128, 128),
        "silver" => rgb(192, 192, 192),
        "transparent" => Some([0.0; 4]),
        _ => None,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The application's adaptive placeholder: a frame filling the box, an icon square in its middle 40%.
    pub(crate) const ADAPTIVE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100%" height="100%">
  <defs>
    <linearGradient id="f" x1="1" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#3F51B5" stop-opacity="0"/>
      <stop offset="1" stop-color="#3F51B5" stop-opacity="0.6"/>
    </linearGradient>
    <clipPath id="c"><rect width="100%" height="100%" rx="13"/></clipPath>
  </defs>
  <rect width="100%" height="100%" rx="13" fill="url(#f)" stroke="#3F51B5" stroke-width="8" clip-path="url(#c)"/>
  <svg x="30%" y="30%" width="40%" height="40%" viewBox="0 0 24 24" preserveAspectRatio="xMidYMid meet">
    <path fill="#3F51B5" d="M13 9H18.5L13 3.5V9ZM6 2H14L20 8V20C20 20.53 19.789 21.039 19.414 21.414C19.039 21.789 18.53 22 18 22H6C5.47 22 4.961 21.789 4.586 21.414C4.211 21.039 4 20.53 4 20V4C4 2.89 4.89 2 6 2ZM6 20H18V12L14 16L12 14L6 20ZM8 9C7.47 9 6.961 9.211 6.586 9.586C6.211 9.961 6 10.47 6 11C6 11.53 6.211 12.039 6.586 12.414C6.961 12.789 7.47 13 8 13C8.53 13 9.039 12.789 9.414 12.414C9.789 12.039 10 11.53 10 11C10 10.47 9.789 9.961 9.414 9.586C9.039 9.211 8.53 9 8 9Z"/>
  </svg>
</svg>"##;

    /// RGBA pixels of the SVG drawn at `w` × `h`.
    fn pixels(svg: &str, w: i32, h: i32) -> Vec<[u8; 4]> {
        let png = fallback_png(svg.as_bytes(), w, h).expect("draws");
        let img = skia_safe::Image::from_encoded(skia_safe::Data::new_copy(&png)).expect("a PNG");
        assert_eq!((img.width(), img.height()), (w, h));
        let info = ImageInfo::new((w, h), ColorType::RGBA8888, AlphaType::Unpremul, None);
        let mut px = vec![0u8; (w * h * 4) as usize];
        assert!(img.read_pixels(&info, &mut px, (w * 4) as usize, (0, 0), skia_safe::image::CachingHint::Allow));
        px.chunks(4).map(|c| [c[0], c[1], c[2], c[3]]).collect()
    }

    #[test]
    fn the_adaptive_placeholder_lays_out_at_every_size() {
        for (w, h) in [(250, 250), (500, 120), (80, 200)] {
            let px = pixels(ADAPTIVE, w, h);
            let at = |x: i32, y: i32| px[(y * w + x) as usize];
            // The icon: the opaque blue in the middle. Its 24-unit box is 40% of the shorter side, centred; the
            // drawn file spans x 4…20 and y 2…22 of it.
            let side = 0.4 * w.min(h) as f32;
            let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
            for y in 8..h - 8 {
                for x in 8..w - 8 {
                    let [r, g, b, a] = at(x, y);
                    if a > 250 && r.abs_diff(0x3F) < 12 && g.abs_diff(0x51) < 12 && b.abs_diff(0xB5) < 12 {
                        (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
                    }
                }
            }
            let (iw, ih) = ((x1 - x0 + 1) as f32, (y1 - y0 + 1) as f32);
            let near = |a: f32, b: f32| (a - b).abs() <= 2.5;
            assert!(near(iw, side * 16.0 / 24.0) && near(ih, side * 20.0 / 24.0), "{w}×{h}: icon {iw}×{ih}, want side {side}");
            assert!(near((x0 + x1) as f32 / 2.0, w as f32 / 2.0) && near((y0 + y1) as f32 / 2.0, h as f32 / 2.0), "{w}×{h}: centred");
            // The frame fills the box: its stroke (inside half, 4 units) along every edge's middle, and nothing
            // outside the rounded corners.
            for (x, y) in [(w / 2, 1), (w / 2, h - 2), (1, h / 2), (w - 2, h / 2)] {
                let [r, g, b, a] = at(x, y);
                assert!(a > 250 && b > r && b > g, "{w}×{h}: frame at {x},{y}: {:?}", [r, g, b, a]);
            }
            assert!(at(0, 0)[3] < 40 && at(w - 1, h - 1)[3] < 40, "{w}×{h}: the corners are round");
            // The gradient: transparent toward the top right, 0.6 toward the bottom left.
            let (tr, bl) = (at(w - 14, 14)[3], at(14, h - 14)[3]);
            assert!(tr < 40 && bl > 100, "{w}×{h}: gradient {tr} {bl}");
        }
    }

    #[test]
    fn lengths_colours_and_transforms() {
        let vp = Vp { w: 200.0, h: 100.0 };
        assert_eq!(length("50%", vp, Axis::X), Some(100.0));
        assert_eq!(length("10%", vp, Axis::Y), Some(10.0));
        assert_eq!(length("1in", vp, Axis::X), Some(96.0));
        assert_eq!(numbers("1-2.5.5,3e1"), [1.0, -2.5, 0.5, 30.0]);
        assert_eq!(colour("#3F51B5"), Some([63.0 / 255.0, 81.0 / 255.0, 181.0 / 255.0, 1.0]));
        assert_eq!(colour("#fff"), Some([1.0; 4]));
        assert_eq!(colour("rgb(255, 0, 0)"), Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(aspect_of("xMinYMax slice"), Some((0.0, 1.0, true)));
        let m = transform("translate(10 20) scale(2)").expect("a transform");
        assert_eq!(m.map_point((1.0, 1.0)), Point::new(12.0, 22.0));
        assert!(!draw(skia_safe::surfaces::raster_n32_premul((4, 4)).unwrap().canvas(), b"<html/>", Rect::from_wh(4.0, 4.0)));
    }

    fn aspect_of(v: &str) -> Option<(f32, f32, bool)> {
        let xml = format!(r#"<svg preserveAspectRatio="{v}"/>"#);
        let d = Document::parse(&xml).unwrap();
        aspect(d.root_element())
    }
}
