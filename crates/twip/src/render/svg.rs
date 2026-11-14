//! SVG pictures (`asvg:svgBlip`): the subset application placeholders and Office icons use. Nested viewports with
//! `viewBox` and `preserveAspectRatio`, percentage lengths, `rect` (with `rx`/`ry`), `circle`, `ellipse`, `line`,
//! `polyline`, `polygon` and `path`, linear and radial gradients (either unit space), clip paths, fill, stroke and
//! opacity, from attributes or `style`. The picture's box is the root viewport, so an SVG with no `viewBox` and
//! percentage geometry lays itself out at whatever size the picture has. Text, `use`, masks, filters, patterns and
//! style sheets aren't drawn.
//!
//! [`walk`] reads the file and replays what it draws into a [`Sink`], in kurbo's geometry; `render::graphics::svg`
//! records it into a picture.

use std::collections::HashMap;

use roxmltree::{Document, Node};
use twip_graphics::kurbo::{self, Affine, BezPath, Cap, Join, Point, Shape};

/// Where an SVG draws, in the picture's space: a backend's canvas.
pub trait Sink {
    /// Saves the transform and the clip, for the matching [`Sink::restore`].
    fn save(&mut self);
    fn restore(&mut self);
    /// Applies `m` to what is drawn from here (inside the current transform).
    fn transform(&mut self, m: Affine);
    /// Intersects the clip with `r` (anti-aliased).
    fn clip_rect(&mut self, r: kurbo::Rect);
    /// Intersects the clip with `path`'s fill (anti-aliased).
    fn clip_path(&mut self, path: &BezPath, even_odd: bool);
    /// Draws into a layer composited at `opacity` by [`Sink::end_layer`].
    fn begin_layer(&mut self, opacity: f32);
    fn end_layer(&mut self);
    fn fill(&mut self, path: &BezPath, even_odd: bool, brush: &Brush);
    fn stroke(&mut self, path: &BezPath, stroke: &kurbo::Stroke, brush: &Brush);
}

/// A fill's or stroke's colour: unpremultiplied RGBA in `0..=1`, its opacity applied.
#[derive(Clone, Debug, PartialEq)]
pub enum Brush {
    Solid([f32; 4]),
    Gradient(SvgGradient),
}

/// A linear or radial gradient: its stops (offset, colour), what it paints past its ends, and the transform from its
/// space into the user space it fills.
#[derive(Clone, Debug, PartialEq)]
pub struct SvgGradient {
    pub kind: GradientKind,
    pub stops: Vec<(f32, [f32; 4])>,
    pub spread: Spread,
    pub transform: Affine,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GradientKind {
    Linear { start: Point, end: Point },
    Radial { centre: Point, radius: f64 },
}

/// `spreadMethod`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spread {
    Pad,
    Reflect,
    Repeat,
}

/// Reads an SVG and draws it into `dst` (the picture's box, which is its viewport) through `sink`; false when the
/// bytes aren't an SVG.
// ponytail: parsed again on every draw; cache the tree per image if large SVGs show up in profiles.
pub fn walk(sink: &mut impl Sink, bytes: &[u8], dst: kurbo::Rect) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else { return false };
    let opts = roxmltree::ParsingOptions { allow_dtd: entities_bounded(text), ..Default::default() };
    let Ok(doc) = Document::parse_with_options(text.trim_start_matches('\u{feff}'), opts) else { return false };
    let root = doc.root_element();
    if root.tag_name().name() != "svg" || deeper_than(&doc, crate::docx::xml::MAX_DEPTH) {
        return false;
    }
    let (w, h) = (dst.width() as f32, dst.height() as f32);
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
    sink.save();
    sink.transform(Affine::translate((dst.x0, dst.y0)));
    sink.clip_rect(kurbo::Rect::new(0.0, 0.0, dst.width(), dst.height()));
    let inner = fit(sink, [0.0, 0.0, w, h], view_box, align);
    svg.children(sink, root, &Style::default(), inner);
    sink.restore();
    true
}

/// Whether the text a doctype's entities expand to stays small, so the doctype may be read (illustration tools
/// declare their namespaces as entities). roxmltree resolves at most 255 references inside each reference, and no
/// entity's value is longer than the doctype, so each `&` expands to at most 256 doctypes.
fn entities_bounded(text: &str) -> bool {
    const MAX_EXPANDED: usize = 1 << 26;
    let Some(start) = text.find("<!DOCTYPE") else { return true };
    let doctype = text[start..].find("]>").unwrap_or(text.len() - start);
    text.matches('&').count().saturating_mul(256).saturating_mul(doctype) <= MAX_EXPANDED
}

/// Whether the tree nests elements deeper than `max`, which the painter's recursion couldn't walk. Counted on the
/// tree, not the text: entities may hold elements.
fn deeper_than(doc: &Document, max: usize) -> bool {
    // In document order every parent comes before its children, so each node's depth is its parent's, already known.
    let mut depth = vec![0usize; doc.descendants().count()];
    for n in doc.descendants() {
        let d = n.parent().map_or(0, |p| depth[p.id().get_usize()]) + usize::from(n.is_element());
        if d > max {
            return true;
        }
        depth[n.id().get_usize()] = d;
    }
    false
}

/// An SVG drawn at `w` × `h` pixels as a PNG: the fallback picture Word writes beside an SVG.
pub fn fallback_png(bytes: &[u8], w: i32, h: i32) -> Option<Vec<u8>> {
    let (w, h) = (w.clamp(1, 2048) as u32, h.clamp(1, 2048) as u32);
    Some(png(w, h, &rgba(bytes, w, h)?))
}

/// An SVG drawn at `w` × `h` pixels by twip-graphics on the CPU: unpremultiplied RGBA, rows packed. `None` when the
/// bytes aren't an SVG.
pub(crate) fn rgba(bytes: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
    use twip_graphics::cpu::{Draw, Format, Frame, Renderer, Target};
    let mut rec = twip_graphics::scene::Recorder::new();
    if !super::graphics::svg::draw(&mut rec, bytes, kurbo::Rect::new(0.0, 0.0, f64::from(w), f64::from(h))) {
        return None;
    }
    let picture = rec.finish();
    let mut px = vec![0u8; w as usize * h as usize * 4];
    let mut target = Target::new(&mut px, w, h, w as usize * 4, Format::Rgba8).ok()?;
    let draws = [Draw { picture: &picture, transform: Affine::IDENTITY }];
    let clear = Some(twip_graphics::paint::Color::TRANSPARENT);
    Renderer::new().render(&mut target, &twip_graphics::Resources::default(), &Frame { clear, draws: &draws });
    for p in px.as_chunks_mut::<4>().0 {
        let a = p[3];
        if a != 0 && a != 255 {
            for c in &mut p[..3] {
                *c = ((u32::from(*c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8;
            }
        }
    }
    Some(px)
}

/// RGBA pixels as a PNG.
pub(crate) fn png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
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
    cap: Cap,
    join: Join,
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
            cap: Cap::Butt,
            join: Join::Miter,
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
            Some("round") => Cap::Round,
            Some("square") => Cap::Square,
            Some(_) => Cap::Butt,
            None => s.cap,
        };
        s.join = match prop(n, "stroke-linejoin") {
            Some("round") => Join::Round,
            Some("bevel") => Join::Bevel,
            Some(_) => Join::Miter,
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
    fn children(&self, c: &mut impl Sink, n: Node<'a, 'i>, st: &Style, vp: Vp) {
        for k in n.children().filter(Node::is_element) {
            self.node(c, k, st, vp);
        }
    }

    fn node(&self, c: &mut impl Sink, n: Node<'a, 'i>, parent: &Style, vp: Vp) {
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
            c.transform(m);
        }
        match name {
            "svg" => {
                let l = |a: &str, axis, d: f32| n.attribute(a).and_then(|v| length(v, vp, axis)).unwrap_or(d);
                let r = [l("x", Axis::X, 0.0), l("y", Axis::Y, 0.0), l("width", Axis::X, vp.w), l("height", Axis::Y, vp.h)];
                if r[2] > 0.0 && r[3] > 0.0 {
                    if prop(n, "overflow").is_none_or(|o| o == "hidden" || o == "clip") {
                        let [x, y, w, h] = r.map(f64::from);
                        c.clip_rect(kurbo::Rect::new(x, y, x + w, y + h));
                    }
                    let inner = fit(c, r, view_box(n), aspect(n));
                    layer(c, opacity, |c| self.children(c, n, &st, inner));
                }
            }
            "g" | "a" | "switch" => {
                if let Some(k) = clip {
                    self.clip(c, k, None, vp);
                }
                layer(c, opacity, |c| self.children(c, n, &st, vp));
            }
            _ => {
                if let Some(path) = shape(n, vp).filter(|_| !st.hidden) {
                    if let Some(k) = clip {
                        self.clip(c, k, Some(path.bounding_box()), vp);
                    }
                    layer(c, opacity, |c| self.paint(c, &path, &st, vp));
                }
            }
        }
        c.restore();
    }

    fn url(&self, v: &str) -> Option<Node<'a, 'i>> {
        let id = v.trim().strip_prefix("url(")?.split(')').next()?.trim().trim_matches(['\'', '"']).trim_start_matches('#');
        self.ids.get(id).copied()
    }

    /// Clips to a `clipPath`: its shapes, in the user space of what refers to it (or its bounding box's).
    fn clip(&self, c: &mut impl Sink, k: Node, bbox: Option<kurbo::Rect>, vp: Vp) {
        if k.tag_name().name() != "clipPath" {
            return;
        }
        let mut path = BezPath::new();
        for s in k.children().filter(Node::is_element) {
            let Some(mut p) = shape(s, vp) else { continue };
            if let Some(m) = s.attribute("transform").and_then(transform) {
                p.apply_affine(m);
            }
            path.extend(p);
        }
        let mut m = Affine::IDENTITY;
        if k.attribute("clipPathUnits") == Some("objectBoundingBox") {
            let Some(bb) = bbox else { return };
            m = unit_box(bb);
        }
        if let Some(t) = k.attribute("transform").and_then(transform) {
            m *= t;
        }
        path.apply_affine(m);
        c.clip_path(&path, prop(k, "clip-rule") == Some("evenodd"));
    }

    fn paint(&self, c: &mut impl Sink, path: &BezPath, st: &Style, vp: Vp) {
        let bbox = path.bounding_box();
        if let Some(b) = st.fill.as_ref().and_then(|f| self.brush(f, st.fill_opacity, bbox, vp)) {
            c.fill(path, st.even_odd, &b);
        }
        if st.stroke_width > 0.0
            && let Some(b) = st.stroke.as_ref().and_then(|f| self.brush(f, st.stroke_opacity, bbox, vp))
        {
            let mut s = kurbo::Stroke::new(f64::from(st.stroke_width)).with_caps(st.cap).with_join(st.join).with_miter_limit(f64::from(st.miter));
            if let Some(d) = &st.dash {
                s = s.with_dashes(0.0, d.iter().map(|v| f64::from(*v)));
            }
            c.stroke(path, &s, &b);
        }
    }

    fn brush(&self, f: &Fill, opacity: f32, bbox: kurbo::Rect, vp: Vp) -> Option<Brush> {
        let solid = |[r, g, b, a]: [f32; 4]| Brush::Solid([r, g, b, a * opacity]);
        Some(match f {
            Fill::Colour(col) => solid(*col),
            Fill::Url(id, fallback) => match self.ids.get(id.as_str()).and_then(|g| self.gradient(*g, opacity, bbox, vp)) {
                Some(g) => Brush::Gradient(g),
                None => solid((*fallback)?),
            },
        })
    }

    /// A linear or radial gradient, its attributes and stops following `href`s, at `opacity`.
    fn gradient(&self, g: Node<'a, 'i>, opacity: f32, bbox: kurbo::Rect, vp: Vp) -> Option<SvgGradient> {
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
        let holder = chain.iter().find(|n| n.children().any(|k| k.tag_name().name() == "stop"))?;
        let mut stops: Vec<(f32, [f32; 4])> = Vec::new();
        for s in holder.children().filter(|k| k.tag_name().name() == "stop") {
            let at = fraction(s.attribute("offset").unwrap_or("0")).unwrap_or(0.0).clamp(0.0, 1.0).max(stops.last().map_or(0.0, |s| s.0));
            let [r, g, b, a] = prop(s, "stop-color").and_then(colour).unwrap_or([0.0, 0.0, 0.0, 1.0]);
            let alpha = prop(s, "stop-opacity").and_then(|v| v.trim().parse::<f32>().ok()).unwrap_or(1.0).clamp(0.0, 1.0);
            stops.push((at, [r, g, b, a * alpha * opacity]));
        }
        if stops.len() == 1 {
            stops.push((1.0, stops[0].1));
        }
        let spread = match attr("spreadMethod") {
            Some("reflect") => Spread::Reflect,
            Some("repeat") => Spread::Repeat,
            _ => Spread::Pad,
        };
        let bounding = attr("gradientUnits") != Some("userSpaceOnUse");
        if bounding && (bbox.width() <= 0.0 || bbox.height() <= 0.0) {
            return None;
        }
        let coord = |name: &str, default: &str, axis| {
            let v = attr(name).unwrap_or(default);
            f64::from(if bounding { fraction(v) } else { length(v, vp, axis) }.unwrap_or(0.0))
        };
        let mut transform = if bounding { unit_box(bbox) } else { Affine::IDENTITY };
        if let Some(t) = attr("gradientTransform").and_then(self::transform) {
            transform *= t;
        }
        let kind = if g.tag_name().name() == "linearGradient" {
            GradientKind::Linear {
                start: Point::new(coord("x1", "0%", Axis::X), coord("y1", "0%", Axis::Y)),
                end: Point::new(coord("x2", "100%", Axis::X), coord("y2", "0%", Axis::Y)),
            }
        } else {
            GradientKind::Radial { centre: Point::new(coord("cx", "50%", Axis::X), coord("cy", "50%", Axis::Y)), radius: coord("r", "50%", Axis::D) }
        };
        Some(SvgGradient { kind, stops, spread, transform })
    }
}

/// `draw` in a layer composited at `opacity`, or directly when it is opaque.
fn layer<S: Sink>(c: &mut S, opacity: f32, draw: impl FnOnce(&mut S)) {
    if opacity >= 1.0 {
        return draw(c);
    }
    c.begin_layer(opacity);
    draw(c);
    c.end_layer();
}

/// The transform mapping the unit square onto a bounding box.
fn unit_box(b: kurbo::Rect) -> Affine {
    Affine::translate((b.x0, b.y0)) * Affine::scale_non_uniform(b.width(), b.height())
}

/// A rect with elliptical corners `rx` × `ry` (already fitted to it), clockwise from the top left's end.
fn rounded(r: kurbo::Rect, rx: f64, ry: f64) -> BezPath {
    if rx <= 0.0 || ry <= 0.0 {
        return r.to_path(0.1);
    }
    // The cubic control distance that best approximates a quarter ellipse, as a fraction of its radii.
    const KAPPA: f64 = 0.552_284_749_830_793_4;
    let (kx, ky) = (rx * KAPPA, ry * KAPPA);
    let mut p = BezPath::new();
    p.move_to((r.x0 + rx, r.y0));
    p.line_to((r.x1 - rx, r.y0));
    p.curve_to((r.x1 - rx + kx, r.y0), (r.x1, r.y0 + ry - ky), (r.x1, r.y0 + ry));
    p.line_to((r.x1, r.y1 - ry));
    p.curve_to((r.x1, r.y1 - ry + ky), (r.x1 - rx + kx, r.y1), (r.x1 - rx, r.y1));
    p.line_to((r.x0 + rx, r.y1));
    p.curve_to((r.x0 + rx - kx, r.y1), (r.x0, r.y1 - ry + ky), (r.x0, r.y1 - ry));
    p.line_to((r.x0, r.y0 + ry));
    p.curve_to((r.x0, r.y0 + ry - ky), (r.x0 + rx - kx, r.y0), (r.x0 + rx, r.y0));
    p.close_path();
    p
}

/// A basic shape's outline in its user space.
fn shape(n: Node, vp: Vp) -> Option<BezPath> {
    let l = |a: &str, axis, d: f32| f64::from(n.attribute(a).and_then(|v| length(v, vp, axis)).unwrap_or(d));
    // Circles and ellipses as cubics within this distance of the true curve, in user units.
    const TOLERANCE: f64 = 0.01;
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
            Some(rounded(kurbo::Rect::new(x, y, x + w, y + h), f64::from(rx).clamp(0.0, w / 2.0), f64::from(ry).clamp(0.0, h / 2.0)))
        }
        "circle" => {
            let r = l("r", Axis::D, 0.0);
            (r > 0.0).then(|| kurbo::Circle::new((l("cx", Axis::X, 0.0), l("cy", Axis::Y, 0.0)), r).to_path(TOLERANCE))
        }
        "ellipse" => {
            let (rx, ry) = (l("rx", Axis::X, 0.0), l("ry", Axis::Y, 0.0));
            let centre = (l("cx", Axis::X, 0.0), l("cy", Axis::Y, 0.0));
            (rx > 0.0 && ry > 0.0).then(|| kurbo::Ellipse::new(centre, (rx, ry), 0.0).to_path(TOLERANCE))
        }
        "line" => {
            let mut p = BezPath::new();
            p.move_to((l("x1", Axis::X, 0.0), l("y1", Axis::Y, 0.0)));
            p.line_to((l("x2", Axis::X, 0.0), l("y2", Axis::Y, 0.0)));
            Some(p)
        }
        k @ ("polyline" | "polygon") => {
            let v = numbers(n.attribute("points")?);
            let pts = v.as_chunks::<2>().0;
            if pts.len() < 2 {
                return None;
            }
            let mut p = BezPath::new();
            for (i, [x, y]) in pts.iter().enumerate() {
                let q = (f64::from(*x), f64::from(*y));
                if i == 0 { p.move_to(q) } else { p.line_to(q) }
            }
            if k == "polygon" {
                p.close_path();
            }
            Some(p)
        }
        "path" => BezPath::from_svg(n.attribute("d")?).ok(),
        _ => None,
    }
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

/// Maps a viewport (`r`: x, y, width, height) to its view box on the sink; the viewport its children see.
fn fit(c: &mut impl Sink, r: [f32; 4], view_box: Option<[f32; 4]>, align: Option<(f32, f32, bool)>) -> Vp {
    let [x, y, w, h] = r;
    c.transform(Affine::translate((f64::from(x), f64::from(y))));
    let Some([vx, vy, vw, vh]) = view_box else { return Vp { w, h } };
    let (mut sx, mut sy) = (w / vw, h / vh);
    let (mut tx, mut ty) = (0.0, 0.0);
    if let Some((ax, ay, slice)) = align {
        let s = if slice { sx.max(sy) } else { sx.min(sy) };
        (sx, sy) = (s, s);
        (tx, ty) = ((w - vw * s) * ax, (h - vh * s) * ay);
    }
    let f = f64::from;
    c.transform(Affine::translate((f(tx), f(ty))) * Affine::scale_non_uniform(f(sx), f(sy)) * Affine::translate((-f(vx), -f(vy))));
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

/// A `transform` list as one transform.
fn transform(s: &str) -> Option<Affine> {
    let mut m = Affine::IDENTITY;
    let mut rest = s.trim();
    while !rest.is_empty() {
        let (name, after) = rest.split_once('(')?;
        let (args, after) = after.split_once(')')?;
        let a: Vec<f64> = numbers(args).into_iter().map(f64::from).collect();
        let arg = |i: usize, d: f64| a.get(i).copied().unwrap_or(d);
        let t = match name.trim().trim_start_matches(',').trim() {
            "matrix" if a.len() == 6 => Affine::new([a[0], a[1], a[2], a[3], a[4], a[5]]),
            "translate" => Affine::translate((arg(0, 0.0), arg(1, 0.0))),
            "scale" => Affine::scale_non_uniform(arg(0, 1.0), arg(1, arg(0, 1.0))),
            "rotate" => Affine::rotate_about(arg(0, 0.0).to_radians(), Point::new(arg(1, 0.0), arg(2, 0.0))),
            "skewX" => Affine::skew(arg(0, 0.0).to_radians().tan(), 0.0),
            "skewY" => Affine::skew(0.0, arg(0, 0.0).to_radians().tan()),
            _ => return None,
        };
        m *= t;
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

    /// RGBA pixels (unpremultiplied) of the SVG drawn at `w` × `h`.
    pub(crate) fn pixels(svg: &str, w: i32, h: i32) -> Vec<[u8; 4]> {
        let px = rgba(svg.as_bytes(), w as u32, h as u32).expect("draws");
        px.chunks(4).map(|c| [c[0], c[1], c[2], c[3]]).collect()
    }

    /// Whether the bytes draw as an SVG.
    fn draws(bytes: &[u8]) -> bool {
        rgba(bytes, 4, 4).is_some()
    }

    /// The sizes the adaptive placeholder is checked at.
    pub(crate) const SIZES: [(i32, i32); 3] = [(250, 250), (500, 120), (80, 200)];

    #[test]
    fn the_adaptive_placeholder_lays_out_at_every_size() {
        for (w, h) in SIZES {
            adaptive_layout(&pixels(ADAPTIVE, w, h), w, h);
        }
    }

    /// Checks the adaptive placeholder drawn at `w` × `h` (`px`, unpremultiplied RGBA) lays out as the application's.
    pub(crate) fn adaptive_layout(px: &[[u8; 4]], w: i32, h: i32) {
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
        assert_eq!(m * Point::new(1.0, 1.0), Point::new(12.0, 22.0));
        assert!(!draws(b"<html/>"));
    }

    #[test]
    fn entity_bombs_and_deep_trees_are_not_drawn() {
        let svg = |body: &str| format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 4 4">{body}</svg>"#);
        // Illustration tools' namespace entities read.
        let named = r#"<!DOCTYPE svg [<!ENTITY ns_svg "http://www.w3.org/2000/svg">]><svg xmlns="&ns_svg;"/>"#;
        assert!(entities_bounded(named) && draws(named.as_bytes()));
        // 64 KB of entity referenced 250 times a reference: the doctype isn't read, so the file isn't drawn.
        let a = "x".repeat(1 << 16);
        let bomb = format!(r#"<!DOCTYPE svg [<!ENTITY a "{a}"><!ENTITY b "{}">]>{}"#, "&a;".repeat(250), svg(&"&b;".repeat(20)));
        assert!(!entities_bounded(&bomb) && !draws(bomb.as_bytes()));
        let deep = svg(&format!("{}{}", "<g>".repeat(10_000), "</g>".repeat(10_000)));
        assert!(!draws(deep.as_bytes()));
    }

    fn aspect_of(v: &str) -> Option<(f32, f32, bool)> {
        let xml = format!(r#"<svg preserveAspectRatio="{v}"/>"#);
        let d = Document::parse(&xml).unwrap();
        aspect(d.root_element())
    }
}
