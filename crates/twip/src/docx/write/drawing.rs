//! Shape saving

use std::ops::Range;

use crate::docx::model::Float;
use crate::drawing::colour::{Colour, ColourBase, ColourMod};
use crate::drawing::geom::Geometry;
use crate::drawing::paint::{BlipFill, BlipMode, Cap, Compound, Dash, EffectList, Fill, GradientKind, GradientPath, Join, Line, LineEnd, TileFlip};
use crate::drawing::text::{AutoFit, BodyProps, VAnchor};
use crate::drawing::units::Pct;
use crate::drawing::{Shape, ShapeKind, Spans};

use super::props::esc;

pub(super) type Edit = (Range<usize>, String);

pub(super) fn changed(was: &Float, now: &Float, text: &str) -> Option<Vec<Edit>> {
    let (Some(a), Some(b), Some(src)) = (was.shape.as_deref(), now.shape.as_deref(), was.src.as_ref()) else { return None };
    let mut edits = shape_changed(a, b, src.start, text).unwrap_or_default();
    link_edits(a, b, text, &mut edits);
    (!edits.is_empty()).then_some(edits)
}

/// A text box's link edits, found in its drawing's `text`: a first box's `wps:txbx` takes its chain's id, a box
/// that goes on from another has its `wps:txbx` replaced by `wps:linkedTxbx`, one that no longer does loses its
/// `wps:linkedTxbx` ([`story_added`] writes its story), and one moved in a chain has its id and place set.
fn link_edits(a: &Shape, b: &Shape, text: &str, out: &mut Vec<Edit>) {
    use crate::drawing::ShapeText as T;
    let (Some(x), Some(y)) = (a.sp(), b.sp()) else { return };
    if (&x.text, x.chain) == (&y.text, y.chain) {
        return;
    }
    let linked = |chain: u32, seq: u32| format!(r#"<wps:linkedTxbx id="{chain}" seq="{seq}"/>"#);
    match (&x.text, &y.text) {
        (T::Story, T::Story) => {
            if let Some((start, end)) = super::find_tag(text, "txbx", 0) {
                let tag = &text[start..end];
                match y.chain {
                    Some(c) => out.push((start..end, set_attr(tag, "id", &c.to_string()))),
                    // A box no longer heading a chain: its id goes.
                    None => {
                        if let Some((.., r)) = crate::docx::binding::tag_attrs(tag).find(|(n, ..)| *n == "id") {
                            let from = tag[..r.start].trim_end().len();
                            out.push((start..end, format!("{}{}", &tag[..from], &tag[r.end..])));
                        }
                    }
                }
            }
        }
        (T::Story, T::Linked { chain, seq }) => {
            if let Some(r) = super::element(text, "txbx") {
                out.push((r, linked(*chain, *seq)));
            }
        }
        (T::Linked { .. }, T::Linked { chain, seq }) => {
            if let Some(r) = super::element(text, "linkedTxbx") {
                out.push((r, linked(*chain, *seq)));
            }
        }
        (T::Linked { .. }, _) => {
            if let Some(r) = super::element(text, "linkedTxbx") {
                out.push((r, String::new()));
            }
        }
        _ => {}
    }
}

/// Where a story a shape gained (typed into a shape without one, or a box no longer going on from another) is written
/// in its drawing: its body properties' place.
pub(super) fn story_added(was: &Float, now: &Float) -> Option<usize> {
    use crate::drawing::ShapeText;
    let (a, b, src) = (was.shape.as_deref()?, now.shape.as_deref()?, was.src.as_ref()?);
    let had = matches!(a.sp()?.text, ShapeText::None | ShapeText::Linked { .. });
    let added = was.text_box.is_none() && now.text_box.is_some() && had && b.sp()?.text == ShapeText::Story;
    added.then(|| a.spans.body.as_ref().map(|r| r.start - src.start)).flatten()
}

/// A story's `wps:txbx` start tag: with its chain's id when it is a chain's first box.
pub(super) fn txbx_open(s: Option<&Shape>) -> String {
    match s.and_then(Shape::sp).and_then(|sp| sp.chain) {
        Some(c) => format!(r#"<wps:txbx id="{c}">"#),
        None => "<wps:txbx>".into(),
    }
}

pub(super) fn is_vml(text: &str) -> bool {
    !text.contains("<wp:") && (text.contains("<v:") || text.contains(":pict"))
}

/// A VML picture's first shape at `w` × `h` points.
pub(super) fn vml_size(text: &str, w: f64, h: f64) -> String {
    let Some((start, end)) = first_vml_shape(text) else { return text.to_string() };
    let pt = |v: f64| format!("{}pt", (v * 20.0).round() / 20.0);
    let tag = &text[start..end];
    let style = css_set(&css_set(&attr(tag, "style").unwrap_or_default(), "width", &pt(w)), "height", &pt(h));
    format!("{}{}{}", &text[..start], set_attr(tag, "style", &style), &text[end..])
}

/// Whether VML says what changed from `a` to `b`: what [`vml_patch`] writes (size, place, turn, flips, solid fills
/// and a shape's lines, a line's width, name, title, hidden, a picture's crop and its brightness, contrast, grayscale and black and
/// white at half). Anything else is written as DrawingML, as Word converts an edited legacy shape.
pub(super) fn vml_says(a: &Shape, b: &Shape) -> bool {
    use crate::drawing::paint::{BlipAdjust as A, Fill as F};
    // A fill or a width cleared isn't written: VML would keep the one it has.
    let solid =
        |f: &Option<F>| matches!(f, Some(F::None)) || matches!(f, Some(F::Solid(c)) if matches!(c.base, ColourBase::Srgb(_)) && c.mods.is_empty());
    let line = |x: &Option<Line>, y: &Option<Line>| {
        x == y
            || y.as_ref().is_some_and(|l| {
                let base = x.clone().unwrap_or_default();
                (l.fill == base.fill || solid(&l.fill))
                    && (l.width == base.width || l.width.is_some())
                    && Line { width: l.width, fill: l.fill.clone(), ..base } == *l
            })
    };
    let adjust = |v: &[A]| v.iter().all(|a| matches!(a, A::Lum { .. } | A::Grayscale | A::BiLevel(Pct(50000))));
    match (&a.kind, &b.kind) {
        (ShapeKind::Sp(x), ShapeKind::Sp(y)) => {
            fn rest(s: &crate::drawing::SpShape) -> impl PartialEq + '_ {
                (&s.geom, &s.effects, &s.body, &s.style, &s.text_path, s.text_box, s.connector, &s.text, s.chain)
            }
            rest(x) == rest(y) && (x.fill == y.fill || solid(&y.fill)) && line(&x.line, &y.line)
        }
        (ShapeKind::Pic(x), ShapeKind::Pic(y)) => {
            (&x.geom, &x.effects, &x.blip.mode, x.blip.image) == (&y.geom, &y.effects, &y.blip.mode, y.blip.image)
                && (x.blip.adjust == y.blip.adjust || adjust(&y.blip.adjust))
                // A picture's outline isn't patched
                && x.line == y.line
        }
        (x, y) => x == y,
    }
}

/// A VML picture's `v:imagedata` as `b` crops and adjusts it.
fn imagedata_patch(tag: &str, b: &crate::drawing::paint::BlipFill) -> String {
    use crate::drawing::paint::BlipAdjust as A;
    let f = |v: f64| format!("{}f", (v * 65536.0).round() as i64);
    let mut tag = tag.to_string();
    for (k, name) in ["cropleft", "croptop", "cropright", "cropbottom"].into_iter().enumerate() {
        if b.src[k].0 != 0 || attr(&tag, name).is_some() {
            tag = set_attr(&tag, name, &f(b.src[k].fraction()));
        }
    }
    let (bright, contrast) = b
        .adjust
        .iter()
        .find_map(|a| if let A::Lum { bright, contrast } = a { Some((bright.fraction(), contrast.fraction())) } else { None })
        .unwrap_or((0.0, 0.0));
    // The contrast slope and half the brightness, as the reader reads them back.
    let gain = if contrast <= 0.0 { 1.0 + contrast } else { 1.0 / (1.0 - contrast).max(1e-3) };
    if gain != 1.0 || attr(&tag, "gain").is_some() {
        tag = set_attr(&tag, "gain", &f(gain));
    }
    if bright != 0.0 || attr(&tag, "blacklevel").is_some() {
        tag = set_attr(&tag, "blacklevel", &f(bright / 2.0));
    }
    for (name, on) in [("grayscale", b.adjust.contains(&A::Grayscale)), ("bilevel", b.adjust.iter().any(|a| matches!(a, A::BiLevel(_))))] {
        if on || attr(&tag, name).is_some() {
            tag = set_attr(&tag, name, if on { "t" } else { "f" });
        }
    }
    tag
}

/// A VML inline picture's text as `b` crops and adjusts it, at `w` × `h`; `None` when VML can't say how it changed.
pub(super) fn vml_picture_patch(text: &str, a: Option<&Shape>, b: Option<&Shape>, w: f64, h: f64) -> Option<String> {
    let x = vml_size(text, w, h);
    let (Some(a), Some(b)) = (a, b) else { return Some(x) };
    if !vml_says(a, b) {
        return None;
    }
    let Some((start, end)) = first_vml_shape(&x) else { return Some(x) };
    let mut tag = x[start..end].to_string();
    let mut style = attr(&tag, "style").unwrap_or_default();
    if a.xfrm.rot != b.xfrm.rot {
        style = css_set(&style, "rotation", &format!("{}", b.xfrm.rot.degrees()));
    }
    if (a.xfrm.flip_h, a.xfrm.flip_v) != (b.xfrm.flip_h, b.xfrm.flip_v) {
        style = css_set(&style, "flip", flip_css(b));
    }
    if a.nv.hidden != b.nv.hidden {
        style = css_set(&style, "visibility", if b.nv.hidden { "hidden" } else { "visible" });
    }
    tag = set_attr(&tag, "style", &style);
    if a.nv.title != b.nv.title {
        tag = set_attr(&tag, "title", &b.nv.title);
    }
    let mut out = format!("{}{tag}{}", &x[..start], &x[end..]);
    if let (Some(pa), Some(pb)) = (a.pic(), b.pic())
        && pa.blip != pb.blip
        && let Some((i0, i1)) = find_imagedata(&out)
    {
        let t = imagedata_patch(&out[i0..i1], &pb.blip);
        out.replace_range(i0..i1, &t);
    }
    Some(out)
}

fn flip_css(b: &Shape) -> &'static str {
    match (b.xfrm.flip_h, b.xfrm.flip_v) {
        (true, true) => "x y",
        (true, false) => "x",
        (false, true) => "y",
        _ => "",
    }
}

/// The first `v:imagedata` start tag's range.
fn find_imagedata(text: &str) -> Option<(usize, usize)> {
    let (open, _) = super::find_tag(text, "imagedata", 0)?;
    let end = open + crate::docx::binding::tag_end(&text[open..])? + 1;
    Some((open, end))
}

/// A VML float's text with `now`'s edits; `None` when VML can't say how it changed ([`vml_says`]).
pub(super) fn vml_patch(text: &str, was: &Float, now: &Float) -> Option<String> {
    use crate::drawing::paint::Fill as F;
    if let (Some(a), Some(b)) = (was.shape.as_deref(), now.shape.as_deref())
        && !vml_says(a, b)
    {
        return None;
    }
    let Some((start, end)) = first_vml_shape(text) else { return Some(text.to_string()) };
    let mut tag = text[start..end].to_string();
    let pt = |v: f64| format!("{}pt", (v * 20.0).round() / 20.0);
    let mut style = attr(&tag, "style").unwrap_or_default();
    let mut set = |k: &str, v: String| style = css_set(&style, k, &v);
    if (was.h_offset, was.v_offset) != (now.h_offset, now.v_offset) {
        set("margin-left", pt(now.h_offset));
        set("margin-top", pt(now.v_offset));
    }
    if (was.w, was.h) != (now.w, now.h) {
        set("width", pt(now.w));
        set("height", pt(now.h));
    }
    let (a, b) = (was.shape.as_deref(), now.shape.as_deref());
    if let (Some(a), Some(b)) = (a, b) {
        if a.xfrm.rot != b.xfrm.rot {
            set("rotation", format!("{}", b.xfrm.rot.degrees()));
        }
        if (a.xfrm.flip_h, a.xfrm.flip_v) != (b.xfrm.flip_h, b.xfrm.flip_v) {
            set("flip", flip_css(b).to_string());
        }
    }
    if (was.z, was.behind) != (now.z, now.behind) {
        let z = i64::from(now.z.max(1)) * if now.behind { -1 } else { 1 };
        set("z-index", z.to_string());
    }
    let nv = |s: Option<&Shape>| s.map(|s| (s.nv.name.clone(), s.nv.title.clone(), s.nv.hidden));
    let (na, nb) = (nv(a), nv(b));
    if let (Some(x), Some((name, title, hidden))) = (&na, &nb)
        && na != nb
    {
        if x.2 != *hidden {
            set("visibility", if *hidden { "hidden" } else { "visible" }.to_string());
        }
        if x.1 != *title {
            tag = set_attr(&tag, "title", title);
        }
        // Its `id` is its name; a linked text box keeps it: the box before it names it so.
        let linked = b.and_then(|s| s.sp()).is_some_and(|sp| matches!(sp.text, crate::drawing::ShapeText::Linked { .. }));
        if x.0 != *name && !linked && !name.is_empty() {
            tag = set_attr(&tag, "id", name);
        }
    }
    tag = set_attr(&tag, "style", &style);
    let hex = |c: &Colour| match c.base {
        ColourBase::Srgb(v) => Some(format!("#{v:06x}")),
        _ => None,
    };
    if let (Some(a), Some(b)) = (a.and_then(|s| s.sp()), b.and_then(|s| s.sp())) {
        if a.fill != b.fill {
            match &b.fill {
                Some(F::Solid(c)) => {
                    if let Some(h) = hex(c) {
                        tag = set_attr(&tag, "fillcolor", &h);
                        tag = set_attr(&tag, "filled", "t");
                    }
                }
                Some(F::None) => tag = set_attr(&tag, "filled", "f"),
                _ => {}
            }
        }
        if a.line != b.line
            && let Some(l) = &b.line
        {
            match &l.fill {
                Some(F::Solid(c)) => {
                    if let Some(h) = hex(c) {
                        tag = set_attr(&tag, "strokecolor", &h);
                        tag = set_attr(&tag, "stroked", "t");
                    }
                }
                Some(F::None) => tag = set_attr(&tag, "stroked", "f"),
                _ => {}
            }
            if let Some(w) = l.width {
                tag = set_attr(&tag, "strokeweight", &pt(w.0 as f64 / 12700.0));
            }
        }
    }
    let mut out = format!("{}{tag}{}", &text[..start], &text[end..]);
    if let (Some(pa), Some(pb)) = (a.and_then(Shape::pic), b.and_then(Shape::pic))
        && pa.blip != pb.blip
        && let Some((i0, i1)) = find_imagedata(&out)
    {
        let t = imagedata_patch(&out[i0..i1], &pb.blip);
        out.replace_range(i0..i1, &t);
    }
    Some(out)
}

fn first_vml_shape(text: &str) -> Option<(usize, usize)> {
    let mut at = 0;
    while let Some(k) = text[at..].find("<v:") {
        let start = at + k;
        let end = start + text[start..].find('>')? + 1;
        if !text[start..].starts_with("<v:shapetype") {
            return Some((start, end));
        }
        at = text[start..].find("</v:shapetype>").map_or(end, |e| start + e);
    }
    None
}

/// Attribute `name`'s value, unescaped ([`set_attr`] escapes what is written back).
fn attr(tag: &str, name: &str) -> Option<String> {
    super::attr_range(tag, name).map(|r| crate::docx::binding::unescape(&tag[r]))
}

fn set_attr(tag: &str, name: &str, value: &str) -> String {
    super::set_attr(tag, name, &esc(value))
}

fn css_set(css: &str, key: &str, value: &str) -> String {
    let mut found = false;
    let mut out: Vec<String> = css
        .split(';')
        .filter(|d| !d.trim().is_empty())
        .map(|d| match d.split_once(':') {
            Some((k, _)) if k.trim().eq_ignore_ascii_case(key) => {
                found = true;
                format!("{key}:{value}")
            }
            _ => d.to_string(),
        })
        .collect();
    if !found {
        out.push(format!("{key}:{value}"));
    }
    out.join(";")
}

pub(super) fn shape_changed(a: &Shape, b: &Shape, base: usize, text: &str) -> Option<Vec<Edit>> {
    let mut edits = Vec::new();
    shape_edits(a, b, base, text, &mut edits);
    (!edits.is_empty()).then_some(edits)
}

pub(super) fn differs(was: &Float, now: &Float) -> bool {
    match (was.shape.as_deref(), now.shape.as_deref()) {
        (Some(a), Some(b)) => (a.xfrm.rot, a.xfrm.flip_h, a.xfrm.flip_v, &a.kind) != (b.xfrm.rot, b.xfrm.flip_h, b.xfrm.flip_v, &b.kind),
        _ => false,
    }
}

/// `src` with the edits made, from the end: of those at one place, a replacement before the insertions there, and
/// insertions in the order given.
pub(super) fn apply(src: &str, edits: Vec<Edit>) -> String {
    let mut edits: Vec<(usize, Edit)> = edits.into_iter().enumerate().collect();
    edits.sort_by_key(|(k, (r, _))| std::cmp::Reverse((r.start, r.end, *k)));
    let mut s = src.to_string();
    for (_, (r, with)) in edits {
        if r.end <= s.len() && r.start <= r.end {
            s.replace_range(r, &with);
        }
    }
    s
}

fn shape_edits(a: &Shape, b: &Shape, base: usize, text: &str, out: &mut Vec<Edit>) {
    shape_edits_in(a, b, base, text, false, out)
}

fn shape_edits_in(a: &Shape, b: &Shape, base: usize, text: &str, is_child: bool, out: &mut Vec<Edit>) {
    let rel = |r: &Range<usize>| r.start.saturating_sub(base)..r.end.saturating_sub(base);
    let sp = &a.spans;
    let frame = |s: &Shape| (s.xfrm.off, s.xfrm.ext, s.group().map(|g| (g.child_off, g.child_ext)));
    if frame(a) != frame(b)
        && let Some(x) = &sp.xfrm
    {
        out.push((rel(x), xfrm_xml(b)));
    } else if (a.xfrm.rot, a.xfrm.flip_h, a.xfrm.flip_v) != (b.xfrm.rot, b.xfrm.flip_h, b.xfrm.flip_v)
        && let Some(x) = &sp.xfrm
    {
        let r = rel(x);
        if let Some(end) = text.get(r.start..).and_then(crate::docx::binding::tag_end).map(|i| r.start + i + 1) {
            let closed = text[..end].ends_with("/>");
            out.push((r.start..end, xfrm_tag(b, closed)));
        }
    }
    // A group child's name, title and hidden mark, in its own `cNvPr` (the drawing's are in its `wp:docPr`).
    if is_child
        && (&a.nv.name, &a.nv.title, a.nv.hidden) != (&b.nv.name, &b.nv.title, b.nv.hidden)
        && let Some(r) = sp.shape.as_ref().map(rel)
        && let Some(open) = text.get(r.clone()).and_then(|t| super::find_tag(t, "cNvPr", 0)).map(|(k, _)| r.start + k)
        && let Some(end) = text.get(open..).and_then(crate::docx::binding::tag_end).map(|e| open + e + 1)
    {
        let mut tag = set_attr(&text[open..end], "name", &b.nv.name);
        if !b.nv.title.is_empty() || super::attr_range(&tag, "title").is_some() {
            tag = set_attr(&tag, "title", &b.nv.title);
        }
        if b.nv.hidden || super::attr_range(&tag, "hidden").is_some() {
            tag = set_attr(&tag, "hidden", if b.nv.hidden { "1" } else { "0" });
        }
        out.push((open..end, tag));
    }
    match (&a.kind, &b.kind) {
        (ShapeKind::Sp(x), ShapeKind::Sp(y)) => {
            if x.geom != y.geom
                && let Some(g) = &sp.geom
            {
                out.push((rel(g), geometry_xml(&y.geom)));
            }
            props_edits(sp, (&x.fill, &y.fill), (&x.line, &y.line), (&x.effects, &y.effects), &rel, text, out);
            if x.body != y.body
                && let Some(r) = &sp.body
            {
                out.push((rel(r), body_xml(&y.body)));
            }
        }
        (ShapeKind::Pic(x), ShapeKind::Pic(y)) => {
            if x.geom != y.geom
                && let Some(g) = &sp.geom
            {
                out.push((rel(g), geometry_xml(&y.geom)));
            }
            if (&x.blip.src, &x.blip.adjust) != (&y.blip.src, &y.blip.adjust)
                && let Some(f) = &sp.blip
                && let Some(t) = text.get(rel(f))
            {
                out.push((rel(f), blip_patch(t, &y.blip)));
            }
            props_edits(sp, (&None, &None), (&x.line, &y.line), (&x.effects, &y.effects), &rel, text, out);
        }
        (ShapeKind::Group(x), ShapeKind::Group(y)) if x.children.len() == y.children.len() => {
            for (ca, cb) in x.children.iter().zip(&y.children) {
                shape_edits_in(ca, cb, base, text, true, out);
            }
        }
        _ => {}
    }
}

fn props_edits(
    sp: &Spans,
    (fa, fb): (&Option<Fill>, &Option<Fill>),
    (la, lb): (&Option<Line>, &Option<Line>),
    (ea, eb): (&Option<EffectList>, &Option<EffectList>),
    rel: &dyn Fn(&Range<usize>) -> Range<usize>,
    text: &str,
    out: &mut Vec<Edit>,
) {
    let at = |r: &Option<Range<usize>>| r.as_ref().map(|r| rel(r).end);
    // The properties' start tag: its range and whether it closes itself.
    let open = sp.sp_pr.as_ref().map(rel).and_then(|r| {
        let end = r.start + crate::docx::binding::tag_end(text.get(r.clone())?)? + 1;
        Some((r.start..end, text[..end].ends_with("/>")))
    });
    let first = open.as_ref().filter(|(_, closed)| !closed).map(|(t, _)| t.end);
    let after = |rs: &[&Option<Range<usize>>]| rs.iter().find_map(|r| at(r)).or(sp.xfrm.as_ref().map(|r| rel(r).end)).or(first);
    // Children an empty `spPr` gains, in schema order: it is written open around them.
    let mut kids = String::new();
    let mut put = |el: &Option<Range<usize>>, before: &[&Option<Range<usize>>], xml: String, out: &mut Vec<Edit>| match el {
        Some(r) => out.push((rel(r), xml)),
        None => match after(before) {
            Some(p) => out.push((p..p, xml)),
            None => kids.push_str(&xml),
        },
    };
    if fa != fb {
        put(&sp.fill, &[&sp.geom], fb.as_ref().map_or(String::new(), fill_xml), out);
    }
    if la != lb {
        put(&sp.line, &[&sp.fill, &sp.geom], lb.as_ref().map_or(String::new(), line_xml), out);
    }
    if ea != eb {
        put(&sp.effects, &[&sp.line, &sp.fill, &sp.geom], eb.as_ref().map_or(String::new(), effects_xml), out);
    }
    if !kids.is_empty()
        && let Some((tag, true)) = open
    {
        let start = &text[tag.start..tag.end - 2];
        let name = start[1..].split(|c: char| c.is_ascii_whitespace()).next().unwrap_or_default();
        out.push((tag, format!("{}>{kids}</{name}>", start.trim_end())));
    }
}

fn xfrm_xml(s: &Shape) -> String {
    let x = &s.xfrm;
    let mut t = xfrm_tag(s, false);
    t.push_str(&format!(r#"<a:off x="{}" y="{}"/><a:ext cx="{}" cy="{}"/>"#, x.off[0].0, x.off[1].0, x.ext[0].0, x.ext[1].0));
    if let Some(g) = s.group() {
        t.push_str(&format!(
            r#"<a:chOff x="{}" y="{}"/><a:chExt cx="{}" cy="{}"/>"#,
            g.child_off[0].0, g.child_off[1].0, g.child_ext[0].0, g.child_ext[1].0
        ));
    }
    t.push_str("</a:xfrm>");
    t
}

fn xfrm_tag(b: &Shape, closed: bool) -> String {
    let mut tag = String::from("<a:xfrm");
    if b.xfrm.rot.0 != 0 {
        tag.push_str(&format!(r#" rot="{}""#, b.xfrm.rot.0));
    }
    if b.xfrm.flip_h {
        tag.push_str(r#" flipH="1""#);
    }
    if b.xfrm.flip_v {
        tag.push_str(r#" flipV="1""#);
    }
    tag.push_str(if closed { "/>" } else { ">" });
    tag
}

fn geometry_xml(g: &Geometry) -> String {
    match g {
        Geometry::Preset { preset, adj } => {
            let av: String = adj.iter().map(|(n, v)| format!(r#"<a:gd name="{}" fmla="val {v}"/>"#, esc(n))).collect();
            format!(r#"<a:prstGeom prst="{}"><a:avLst>{av}</a:avLst></a:prstGeom>"#, preset.name())
        }
        Geometry::Custom(_) => String::new(),
    }
}

fn pct(v: crate::drawing::units::Pct) -> i32 {
    v.0
}

pub fn colour_xml(c: &Colour) -> String {
    use ColourMod::*;
    let mods: String = c
        .mods
        .iter()
        .map(|m| {
            let (name, val) = match *m {
                Tint(p) => ("tint", Some(pct(p))),
                Shade(p) => ("shade", Some(pct(p))),
                Comp => ("comp", None),
                Inv => ("inv", None),
                Gray => ("gray", None),
                Alpha(p) => ("alpha", Some(pct(p))),
                AlphaOff(p) => ("alphaOff", Some(pct(p))),
                AlphaMod(p) => ("alphaMod", Some(pct(p))),
                Hue(a) => ("hue", Some(a.0)),
                HueOff(a) => ("hueOff", Some(a.0)),
                HueMod(p) => ("hueMod", Some(pct(p))),
                Sat(p) => ("sat", Some(pct(p))),
                SatOff(p) => ("satOff", Some(pct(p))),
                SatMod(p) => ("satMod", Some(pct(p))),
                Lum(p) => ("lum", Some(pct(p))),
                LumOff(p) => ("lumOff", Some(pct(p))),
                LumMod(p) => ("lumMod", Some(pct(p))),
                Red(p) => ("red", Some(pct(p))),
                RedOff(p) => ("redOff", Some(pct(p))),
                RedMod(p) => ("redMod", Some(pct(p))),
                Green(p) => ("green", Some(pct(p))),
                GreenOff(p) => ("greenOff", Some(pct(p))),
                GreenMod(p) => ("greenMod", Some(pct(p))),
                Blue(p) => ("blue", Some(pct(p))),
                BlueOff(p) => ("blueOff", Some(pct(p))),
                BlueMod(p) => ("blueMod", Some(pct(p))),
                Gamma => ("gamma", None),
                InvGamma => ("invGamma", None),
            };
            match val {
                Some(v) => format!(r#"<a:{name} val="{v}"/>"#),
                None => format!("<a:{name}/>"),
            }
        })
        .collect();
    let (tag, attrs) = match &c.base {
        ColourBase::Srgb(v) => ("srgbClr", format!(r#" val="{v:06X}""#)),
        ColourBase::ScRgb([r, g, b]) => ("scrgbClr", format!(r#" r="{}" g="{}" b="{}""#, r.0, g.0, b.0)),
        ColourBase::Hsl { hue, sat, lum } => ("hslClr", format!(r#" hue="{}" sat="{}" lum="{}""#, hue.0, sat.0, lum.0)),
        ColourBase::Sys { name, last } => ("sysClr", format!(r#" val="{}" lastClr="{last:06X}""#, esc(name))),
        ColourBase::Preset { name, .. } => ("prstClr", format!(r#" val="{}""#, esc(name))),
        ColourBase::Scheme(s) => ("schemeClr", format!(r#" val="{}""#, s.name())),
    };
    if mods.is_empty() { format!("<a:{tag}{attrs}/>") } else { format!("<a:{tag}{attrs}>{mods}</a:{tag}>") }
}

fn rect4(name: &str, r: &[crate::drawing::units::Pct; 4]) -> String {
    if r.iter().all(|v| v.0 == 0) {
        return format!("<a:{name}/>");
    }
    format!(r#"<a:{name} l="{}" t="{}" r="{}" b="{}"/>"#, r[0].0, r[1].0, r[2].0, r[3].0)
}

fn flip(f: TileFlip) -> &'static str {
    match f {
        TileFlip::None => "none",
        TileFlip::X => "x",
        TileFlip::Y => "y",
        TileFlip::Xy => "xy",
    }
}

pub fn fill_xml(f: &Fill) -> String {
    match f {
        Fill::None => "<a:noFill/>".into(),
        Fill::Solid(c) => format!("<a:solidFill>{}</a:solidFill>", colour_xml(c)),
        Fill::Gradient(g) => {
            let stops: String = g.stops.iter().map(|(p, c)| format!(r#"<a:gs pos="{}">{}</a:gs>"#, p.0, colour_xml(c))).collect();
            let shade = match g.kind {
                GradientKind::Linear { angle, scaled } => format!(r#"<a:lin ang="{}" scaled="{}"/>"#, angle.0, u8::from(scaled)),
                GradientKind::Path { path, focus } => {
                    let p = match path {
                        GradientPath::Circle => "circle",
                        GradientPath::Rect => "rect",
                        GradientPath::Shape => "shape",
                    };
                    format!(r#"<a:path path="{p}">{}</a:path>"#, rect4("fillToRect", &focus))
                }
            };
            let tile = if g.tile.iter().any(|v| v.0 != 0) { rect4("tileRect", &g.tile) } else { String::new() };
            let fl = if g.flip == TileFlip::None { String::new() } else { format!(r#" flip="{}""#, flip(g.flip)) };
            format!(r#"<a:gradFill{fl} rotWithShape="{}"><a:gsLst>{stops}</a:gsLst>{shade}{tile}</a:gradFill>"#, u8::from(g.rot_with_shape))
        }
        Fill::Pattern { preset, fg, bg } => {
            format!(r#"<a:pattFill prst="{}"><a:fgClr>{}</a:fgClr><a:bgClr>{}</a:bgClr></a:pattFill>"#, preset.name(), colour_xml(fg), colour_xml(bg))
        }
        Fill::Blip(b) => blip_xml(b, "a:blipFill"),
        Fill::Group => "<a:grpFill/>".into(),
    }
}

fn blip_xml(b: &BlipFill, tag: &str) -> String {
    let src = if b.src.iter().any(|v| v.0 != 0) { rect4("srcRect", &b.src) } else { String::new() };
    let mode = match &b.mode {
        BlipMode::Stretch(f) => format!("<a:stretch>{}</a:stretch>", rect4("fillRect", f)),
        BlipMode::Tile { tx, ty, sx, sy, flip: fl, align } => {
            format!(r#"<a:tile tx="{}" ty="{}" sx="{}" sy="{}" flip="{}" algn="{}"/>"#, tx.0, ty.0, sx.0, sy.0, flip(*fl), align.name())
        }
    };
    let embed = b.image.map_or(String::new(), |i| format!(r#" r:embed="{}{i}""#, super::NEW_IMAGE));
    let adjust = adjust_xml(&b.adjust);
    let blip = if adjust.is_empty() { format!("<a:blip{embed}/>") } else { format!("<a:blip{embed}>{adjust}</a:blip>") };
    format!(r#"<{tag} rotWithShape="{}">{blip}{src}{mode}</{tag}>"#, u8::from(b.rot_with_shape))
}

/// A blip's picture adjustments.
fn adjust_xml(adjust: &[crate::drawing::paint::BlipAdjust]) -> String {
    use crate::drawing::paint::BlipAdjust;
    adjust
        .iter()
        .map(|a| match a {
            BlipAdjust::AlphaModFix(p) => format!(r#"<a:alphaModFix amt="{}"/>"#, p.0),
            BlipAdjust::Grayscale => "<a:grayscl/>".into(),
            BlipAdjust::Lum { bright, contrast } => format!(r#"<a:lum bright="{}" contrast="{}"/>"#, bright.0, contrast.0),
            BlipAdjust::BiLevel(p) => format!(r#"<a:biLevel thresh="{}"/>"#, p.0),
            BlipAdjust::Duotone([a, b]) => format!("<a:duotone>{}{}</a:duotone>", colour_xml(a), colour_xml(b)),
            BlipAdjust::ClrChange { from, to, use_a } => format!(
                r#"<a:clrChange{}><a:clrFrom>{}</a:clrFrom><a:clrTo>{}</a:clrTo></a:clrChange>"#,
                if *use_a { "" } else { r#" useA="0""# },
                colour_xml(from),
                colour_xml(to)
            ),
        })
        .collect()
}

/// A picture's `pic:blipFill` as read (`src`) with its crop and adjustments as `b` has them: the blip's start tag (its
/// relationship) and extensions kept.
fn blip_patch(src: &str, b: &BlipFill) -> String {
    let mut s = src.to_string();
    if let Some(r) = super::element(&s, "srcRect") {
        s.replace_range(r, "");
    }
    let Some(r) = super::element(&s, "blip") else { return s };
    let el = s[r.clone()].to_string();
    let Some(open_end) = el.find('>').map(|k| k + 1) else { return s };
    let name = el[1..].split(|c: char| c.is_whitespace() || c == '/' || c == '>').next().unwrap_or("a:blip").to_string();
    let open = el[..open_end].trim_end_matches("/>").trim_end_matches('>').to_string();
    let ext = super::element(&el, "extLst").map(|e| el[e].to_string()).unwrap_or_default();
    let inner = adjust_xml(&b.adjust) + &ext;
    let blip = if inner.is_empty() { format!("{open}/>") } else { format!("{open}>{inner}</{name}>") };
    let crop = if b.src.iter().any(|v| v.0 != 0) { rect4("srcRect", &b.src) } else { String::new() };
    s.replace_range(r, &(blip + &crop));
    s
}

fn end_xml(tag: &str, e: &Option<LineEnd>) -> String {
    use crate::drawing::paint::{EndKind, EndSize};
    let Some(e) = e else { return String::new() };
    let size = |s: EndSize| match s {
        EndSize::Small => "sm",
        EndSize::Medium => "med",
        EndSize::Large => "lg",
    };
    let kind = match e.kind {
        EndKind::Triangle => "triangle",
        EndKind::Stealth => "stealth",
        EndKind::Diamond => "diamond",
        EndKind::Oval => "oval",
        EndKind::Arrow => "arrow",
    };
    format!(r#"<a:{tag} type="{kind}" w="{}" len="{}"/>"#, size(e.width), size(e.length))
}

pub fn line_xml(l: &Line) -> String {
    let mut attrs = String::new();
    if let Some(w) = l.width {
        attrs.push_str(&format!(r#" w="{}""#, w.0));
    }
    if let Some(c) = l.cap {
        attrs.push_str(match c {
            Cap::Round => r#" cap="rnd""#,
            Cap::Square => r#" cap="sq""#,
            Cap::Flat => r#" cap="flat""#,
        });
    }
    if let Some(c) = l.compound {
        attrs.push_str(match c {
            Compound::Single => r#" cmpd="sng""#,
            Compound::Double => r#" cmpd="dbl""#,
            Compound::ThickThin => r#" cmpd="thickThin""#,
            Compound::ThinThick => r#" cmpd="thinThick""#,
            Compound::Triple => r#" cmpd="tri""#,
        });
    }
    if let Some(i) = l.inset {
        attrs.push_str(if i { r#" algn="in""# } else { r#" algn="ctr""# });
    }
    let fill = l.fill.as_ref().map_or(String::new(), fill_xml);
    let dash = match &l.dash {
        None => String::new(),
        Some(Dash::Preset(p)) => format!(r#"<a:prstDash val="{}"/>"#, p.name()),
        Some(Dash::Custom(ds)) => {
            format!("<a:custDash>{}</a:custDash>", ds.iter().map(|(d, s)| format!(r#"<a:ds d="{}" sp="{}"/>"#, d.0, s.0)).collect::<String>())
        }
    };
    let join = match l.join {
        None => String::new(),
        Some(Join::Round) => "<a:round/>".into(),
        Some(Join::Bevel) => "<a:bevel/>".into(),
        Some(Join::Miter(lim)) => format!(r#"<a:miter lim="{}"/>"#, lim.0),
    };
    format!("<a:ln{attrs}>{fill}{dash}{join}{}{}</a:ln>", end_xml("headEnd", &l.head), end_xml("tailEnd", &l.tail))
}

pub fn effects_xml(e: &EffectList) -> String {
    let mut s = String::from("<a:effectLst>");
    if let Some(g) = &e.blur {
        s.push_str(&format!(r#"<a:blur rad="{}"/>"#, g.0));
    }
    if let Some(g) = &e.glow {
        s.push_str(&format!(r#"<a:glow rad="{}">{}</a:glow>"#, g.radius.0, colour_xml(&g.colour)));
    }
    if let Some(i) = &e.inner {
        s.push_str(&format!(r#"<a:innerShdw blurRad="{}" dist="{}" dir="{}">{}</a:innerShdw>"#, i.blur.0, i.dist.0, i.dir.0, colour_xml(&i.colour)));
    }
    if let Some(o) = &e.outer {
        s.push_str(&format!(
            r#"<a:outerShdw blurRad="{}" dist="{}" dir="{}" sx="{}" sy="{}" kx="{}" ky="{}" algn="{}" rotWithShape="{}">{}</a:outerShdw>"#,
            o.blur.0,
            o.dist.0,
            o.dir.0,
            o.sx.0,
            o.sy.0,
            o.kx.0,
            o.ky.0,
            o.align.name(),
            u8::from(o.rot_with_shape),
            colour_xml(&o.colour)
        ));
    }
    if let Some(p) = &e.preset {
        s.push_str(&format!(r#"<a:prstShdw prst="shdw{}" dist="{}" dir="{}">{}</a:prstShdw>"#, p.preset, p.dist.0, p.dir.0, colour_xml(&p.colour)));
    }
    if let Some(r) = &e.reflection {
        s.push_str(&format!(
            r#"<a:reflection blurRad="{}" stA="{}" stPos="{}" endA="{}" endPos="{}" dist="{}" dir="{}" fadeDir="{}" sx="{}" sy="{}" kx="{}" ky="{}" algn="{}" rotWithShape="{}"/>"#,
            r.blur.0,
            r.start_alpha.0,
            r.start_pos.0,
            r.end_alpha.0,
            r.end_pos.0,
            r.dist.0,
            r.dir.0,
            r.fade_dir.0,
            r.sx.0,
            r.sy.0,
            r.kx.0,
            r.ky.0,
            r.align.name(),
            u8::from(r.rot_with_shape)
        ));
    }
    if let Some(r) = e.soft_edge {
        s.push_str(&format!(r#"<a:softEdge rad="{}"/>"#, r.0));
    }
    s.push_str("</a:effectLst>");
    s
}

// Text effects

/// A colour in Word 2010's text effects (`w14:`): RGB or a theme colour, with its changes. `{w14}` stands for the
/// prefix ([`super::props::Cx::w14`] fills it in).
fn w14_colour_xml(c: &Colour) -> String {
    let (tag, val) = match &c.base {
        ColourBase::Scheme(s) => ("schemeClr", s.name()),
        b => {
            let plain = Colour { base: b.clone(), mods: Vec::new() };
            let rgb = plain.resolve(&crate::drawing::theme::Theme::default(), &crate::drawing::theme::ColourMap::default(), None).to_rgb();
            ("srgbClr", format!("{rgb:06X}"))
        }
    };
    let mods: String = c
        .mods
        .iter()
        .filter_map(|m| {
            let (name, v) = match *m {
                ColourMod::Tint(p) => ("tint", p.0),
                ColourMod::Shade(p) => ("shade", p.0),
                ColourMod::Alpha(p) => ("alpha", p.0),
                ColourMod::HueMod(p) => ("hueMod", p.0),
                ColourMod::Sat(p) => ("sat", p.0),
                ColourMod::SatOff(p) => ("satOff", p.0),
                ColourMod::SatMod(p) => ("satMod", p.0),
                ColourMod::Lum(p) => ("lum", p.0),
                ColourMod::LumOff(p) => ("lumOff", p.0),
                ColourMod::LumMod(p) => ("lumMod", p.0),
                _ => return None,
            };
            Some(format!(r#"<{{w14}}{name} {{w14}}val="{v}"/>"#))
        })
        .collect();
    let val = esc(&val);
    if mods.is_empty() {
        format!(r#"<{{w14}}{tag} {{w14}}val="{val}"/>"#)
    } else {
        format!(r#"<{{w14}}{tag} {{w14}}val="{val}">{mods}</{{w14}}{tag}>"#)
    }
}

/// A text fill or a text outline's fill (`w14:noFill`, `w14:solidFill`, `w14:gradFill`), `{w14}` for the prefix.
fn w14_fill_xml(f: &Fill) -> String {
    match f {
        Fill::Solid(c) => format!("<{{w14}}solidFill>{}</{{w14}}solidFill>", w14_colour_xml(c)),
        Fill::Gradient(g) => {
            let stops: String =
                g.stops.iter().map(|(p, c)| format!(r#"<{{w14}}gs {{w14}}pos="{}">{}</{{w14}}gs>"#, p.0, w14_colour_xml(c))).collect();
            let shade = match g.kind {
                GradientKind::Linear { angle, scaled } => format!(r#"<{{w14}}lin {{w14}}ang="{}" {{w14}}scaled="{}"/>"#, angle.0, u8::from(scaled)),
                GradientKind::Path { path, focus } => {
                    let p = match path {
                        GradientPath::Circle => "circle",
                        GradientPath::Rect => "rect",
                        GradientPath::Shape => "shape",
                    };
                    let [l, t, r, b] = focus.map(|v| v.0);
                    format!(
                        r#"<{{w14}}path {{w14}}path="{p}"><{{w14}}fillToRect {{w14}}l="{l}" {{w14}}t="{t}" {{w14}}r="{r}" {{w14}}b="{b}"/></{{w14}}path>"#
                    )
                }
            };
            format!("<{{w14}}gradFill><{{w14}}gsLst>{stops}</{{w14}}gsLst>{shade}</{{w14}}gradFill>")
        }
        _ => "<{w14}noFill/>".into(),
    }
}

/// A part's element of Word 2010's namespace from its attributes and content.
fn w14_el(cx: &super::props::Cx, local: &str, attrs: &[(&str, String)], inner: &str) -> String {
    let attrs: Vec<(&str, &str)> = attrs.iter().map(|(k, v)| (*k, v.as_str())).collect();
    cx.w14(local, &attrs, inner)
}

/// `w14:textFill`, in the part's prefix.
pub(crate) fn text_fill_xml(cx: &super::props::Cx, f: &Fill) -> String {
    w14_el(cx, "textFill", &[], &w14_fill_xml(f))
}

/// `w14:textOutline`, in the part's prefix.
pub(crate) fn text_outline_xml(cx: &super::props::Cx, l: &Line) -> String {
    let cap = match l.cap.unwrap_or(Cap::Flat) {
        Cap::Round => "rnd",
        Cap::Square => "sq",
        Cap::Flat => "flat",
    };
    let cmpd = match l.compound.unwrap_or_default() {
        Compound::Single => "sng",
        Compound::Double => "dbl",
        Compound::ThickThin => "thickThin",
        Compound::ThinThick => "thinThick",
        Compound::Triple => "tri",
    };
    let attrs = [
        ("w", l.width.map_or(9525, |w| w.0).to_string()),
        ("cap", cap.into()),
        ("cmpd", cmpd.into()),
        ("algn", if l.inset == Some(true) { "in" } else { "ctr" }.into()),
    ];
    let mut inner = l.fill.as_ref().map_or_else(|| "<{w14}noFill/>".into(), w14_fill_xml);
    if let Some(Dash::Preset(d)) = &l.dash {
        inner.push_str(&format!(r#"<{{w14}}prstDash {{w14}}val="{}"/>"#, d.name()));
    }
    inner.push_str(&match l.join {
        Some(Join::Bevel) => "<{w14}bevel/>".to_string(),
        Some(Join::Miter(lim)) => format!(r#"<{{w14}}miter {{w14}}lim="{}"/>"#, lim.0),
        _ => "<{w14}round/>".to_string(),
    });
    w14_el(cx, "textOutline", &attrs, &inner)
}

/// `w14:shadow`, in the part's prefix.
pub(crate) fn text_shadow_xml(cx: &super::props::Cx, s: &crate::drawing::paint::OuterShadow) -> String {
    let attrs = [
        ("blurRad", s.blur.0.to_string()),
        ("dist", s.dist.0.to_string()),
        ("dir", s.dir.0.to_string()),
        ("sx", s.sx.0.to_string()),
        ("sy", s.sy.0.to_string()),
        ("kx", s.kx.0.to_string()),
        ("ky", s.ky.0.to_string()),
        ("algn", s.align.name().to_string()),
    ];
    w14_el(cx, "shadow", &attrs, &w14_colour_xml(&s.colour))
}

/// `w14:glow`, in the part's prefix.
pub(crate) fn text_glow_xml(cx: &super::props::Cx, g: &crate::drawing::paint::Glow) -> String {
    w14_el(cx, "glow", &[("rad", g.radius.0.to_string())], &w14_colour_xml(&g.colour))
}

/// `w14:reflection`, in the part's prefix.
pub(crate) fn text_reflection_xml(cx: &super::props::Cx, r: &crate::drawing::paint::Reflection) -> String {
    let attrs = [
        ("blurRad", r.blur.0.to_string()),
        ("stA", r.start_alpha.0.to_string()),
        ("stPos", r.start_pos.0.to_string()),
        ("endA", r.end_alpha.0.to_string()),
        ("endPos", r.end_pos.0.to_string()),
        ("dist", r.dist.0.to_string()),
        ("dir", r.dir.0.to_string()),
        ("fadeDir", r.fade_dir.0.to_string()),
        ("sx", r.sx.0.to_string()),
        ("sy", r.sy.0.to_string()),
        ("kx", r.kx.0.to_string()),
        ("ky", r.ky.0.to_string()),
        ("algn", r.align.name().to_string()),
    ];
    w14_el(cx, "reflection", &attrs, "")
}

fn body_xml(b: &BodyProps) -> String {
    let anchor = match b.anchor {
        VAnchor::Top | VAnchor::Other => "t",
        VAnchor::Centre => "ctr",
        VAnchor::Bottom => "b",
    };
    let fit = match b.autofit {
        AutoFit::None => "<a:noAutofit/>".to_string(),
        AutoFit::Shape => "<a:spAutoFit/>".to_string(),
        AutoFit::Normal { font_scale, line_reduction } => {
            format!(r#"<a:normAutofit fontScale="{}" lnSpcReduction="{}"/>"#, font_scale.0, line_reduction.0)
        }
    };
    let up = if b.upright { r#" upright="1""# } else { "" };
    let cols = if b.columns.0 > 1 { format!(r#" numCol="{}" spcCol="{}""#, b.columns.0, b.columns.1.0) } else { String::new() };
    format!(
        r#"<wps:bodyPr rot="{}" vert="{}" wrap="{}" lIns="{}" tIns="{}" rIns="{}" bIns="{}"{cols} anchor="{anchor}" anchorCtr="0"{up}>{fit}</wps:bodyPr>"#,
        b.rot.0,
        b.direction.name(),
        if b.no_wrap { "none" } else { "square" },
        b.insets[0].0,
        b.insets[1].0,
        b.insets[2].0,
        b.insets[3].0,
    )
}

const WPS: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingShape";
const WPG: &str = "http://schemas.microsoft.com/office/word/2010/wordprocessingGroup";
const PIC: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

pub(super) type Stories<'a> = &'a dyn Fn(&[u16]) -> Option<String>;

pub(super) fn graphic_data(s: &Shape, txbx: Stories, w: &str) -> String {
    let (uri, ns) = match &s.kind {
        ShapeKind::Group(_) => (WPG, format!(r#" xmlns:wpg="{WPG}" xmlns:wps="{WPS}" xmlns:pic="{PIC}" xmlns:r="{R}""#)),
        ShapeKind::Pic(_) => (PIC, format!(r#" xmlns:pic="{PIC}" xmlns:r="{R}""#)),
        _ => (WPS, format!(r#" xmlns:wps="{WPS}" xmlns:r="{R}""#)),
    };
    format!(r#"<a:graphicData uri="{uri}">{}</a:graphicData>"#, shape_xml(s, &mut Vec::new(), txbx, w, Some(&ns)))
}

fn shape_xml(s: &Shape, path: &mut Vec<u16>, txbx: Stories, w: &str, ns: Option<&str>) -> String {
    let top = path.is_empty();
    let ns = ns.unwrap_or("");
    let xfrm = |_: Option<&crate::drawing::GroupShape>| xfrm_xml(s);
    let name = esc(&s.nv.name);
    let hidden = if s.nv.hidden { r#" hidden="1""# } else { "" };
    let descr = if s.nv.descr.is_empty() { String::new() } else { format!(r#" descr="{}""#, esc(&s.nv.descr)) };
    let descr = if s.nv.title.is_empty() { descr } else { format!(r#"{descr} title="{}""#, esc(&s.nv.title)) };
    let opt = |v: Option<String>| v.unwrap_or_default();
    match &s.kind {
        ShapeKind::Sp(sp) => {
            let nv = if top { String::new() } else { format!(r#"<wps:cNvPr id="{}" name="{name}"{descr}{hidden}/>"#, s.nv.id) };
            let kind = match (sp.connector, sp.text_box) {
                (true, _) => "<wps:cNvCnPr/>",
                (false, true) => r#"<wps:cNvSpPr txBox="1"/>"#,
                _ => "<wps:cNvSpPr/>",
            };
            let props = format!(
                "<wps:spPr>{}{}{}{}{}</wps:spPr>",
                xfrm(None),
                geometry_xml(&sp.geom),
                opt(sp.fill.as_ref().map(fill_xml)),
                opt(sp.line.as_ref().map(line_xml)),
                opt(sp.effects.as_ref().map(effects_xml)),
            );
            let style = opt(sp.style.as_ref().map(style_xml));
            let text = match (&sp.text, txbx(path).filter(|_| sp.text == crate::drawing::ShapeText::Story)) {
                (_, Some(t)) => format!("{}<{w}txbxContent>{t}</{w}txbxContent></wps:txbx>", txbx_open(Some(s))),
                (crate::drawing::ShapeText::Linked { chain, seq }, None) => format!(r#"<wps:linkedTxbx id="{chain}" seq="{seq}"/>"#),
                _ => String::new(),
            };
            let body = if sp.connector { "<wps:bodyPr/>".to_string() } else { body_xml(&sp.body) };
            format!("<wps:wsp{ns}>{nv}{kind}{props}{style}{text}{body}</wps:wsp>")
        }
        ShapeKind::Pic(p) => format!(
            r#"<pic:pic{ns}><pic:nvPicPr><pic:cNvPr id="{}" name="{name}"{descr}{hidden}/><pic:cNvPicPr/></pic:nvPicPr>{}<pic:spPr>{}{}{}{}</pic:spPr></pic:pic>"#,
            if top { 0 } else { s.nv.id },
            blip_xml(&p.blip, "pic:blipFill"),
            xfrm(None),
            geometry_xml(&p.geom),
            opt(p.line.as_ref().map(line_xml)),
            opt(p.effects.as_ref().map(effects_xml)),
        ),
        ShapeKind::Group(g) => {
            let (tag, nv) = if top {
                ("wpg:wgp", String::new())
            } else {
                ("wpg:grpSp", format!(r#"<wpg:cNvPr id="{}" name="{name}"{descr}{hidden}/>"#, s.nv.id))
            };
            let mut kids = String::new();
            for (i, c) in g.children.iter().enumerate() {
                path.push(i as u16);
                kids.push_str(&shape_xml(c, path, txbx, w, None));
                path.pop();
            }
            let fill = opt(g.fill.as_ref().map(fill_xml));
            format!("<{tag}{ns}>{nv}<wpg:cNvGrpSpPr/><wpg:grpSpPr>{}{fill}</wpg:grpSpPr>{kids}</{tag}>", xfrm(Some(g)))
        }
        ShapeKind::Raw(_) => String::new(),
    }
}

fn style_xml(st: &crate::drawing::paint::StyleRefs) -> String {
    let r = |tag: &str, v: &Option<(u32, Option<Colour>)>| match v {
        Some((idx, c)) => format!(r#"<a:{tag} idx="{idx}">{}</a:{tag}>"#, c.as_ref().map(colour_xml).unwrap_or_default()),
        None => format!(r#"<a:{tag} idx="0"/>"#),
    };
    let font = format!(r#"<a:fontRef idx="minor">{}</a:fontRef>"#, st.font.as_ref().map(colour_xml).unwrap_or_default());
    format!("<wps:style>{}{}{}{font}</wps:style>", r("lnRef", &st.line), r("fillRef", &st.fill), r("effectRef", &st.effect))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawing::colour::SchemeSlot;
    use crate::drawing::units::Pct;

    #[test]
    fn fills_and_lines_serialise_as_they_parse() {
        let f = Fill::Solid(Colour::scheme(SchemeSlot::Accent(1)).with(ColourMod::LumMod(Pct(75000))));
        let xml = fill_xml(&f);
        assert_eq!(xml, r#"<a:solidFill><a:schemeClr val="accent1"><a:lumMod val="75000"/></a:schemeClr></a:solidFill>"#);
        let wrapped = format!(r#"<r xmlns:a="{}">{xml}</r>"#, crate::docx::xml::A);
        let d = roxmltree::Document::parse(&wrapped).unwrap();
        assert_eq!(Fill::child_of(d.root_element(), &mut |_| None), Some(f));
        let l = Line {
            width: Some(crate::drawing::units::Emu(12700)),
            fill: Some(Fill::Solid(Colour::srgb(0xFF0000))),
            dash: Some(Dash::Preset(crate::drawing::paint::PresetDash::Dash)),
            ..Line::default()
        };
        let wrapped = format!(r#"<r xmlns:a="{}">{}</r>"#, crate::docx::xml::A, line_xml(&l));
        let d = roxmltree::Document::parse(&wrapped).unwrap();
        assert_eq!(Line::parse(d.root_element().first_element_child().unwrap()), l);
    }

    #[test]
    fn a_vml_style_is_written_back_escaped_once() {
        let x = vml_size(r#"<w:pict><v:shape style="width:1pt;height:1pt;mso-note:a&amp;b"/></w:pict>"#, 2.0, 3.0);
        assert!(x.contains("mso-note:a&amp;b") && x.contains("width:2pt"), "{x}");
    }

    #[test]
    fn imagedata_is_found_whatever_follows_its_name() {
        let x = "<v:shape><v:imagedata\n r:id=\"rId1\"/></v:shape>";
        assert_eq!(find_imagedata(x), Some((9, x.len() - "</v:shape>".len())));
        assert_eq!(find_imagedata("<v:shape><v:imagedataX/></v:shape>"), None);
    }

    #[test]
    fn edits_apply_last_first() {
        assert_eq!(apply("abcdef", vec![(1..2, "X".into()), (4..4, "Y".into())]), "aXcdYef");
    }
}

#[cfg(test)]
mod round_trip {
    use std::io::Read;

    use crate::docx::model::{Block, Inline};
    use crate::drawing::colour::Colour;
    use crate::drawing::paint::Fill;

    const NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape" xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" xmlns:v="urn:schemas-microsoft-com:vml""#;

    const DRAWING: &str = r#"<w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="1" behindDoc="0" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="page"><wp:posOffset>914400</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page"><wp:posOffset>914400</wp:posOffset></wp:positionV><wp:extent cx="1143000" cy="762000"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:wrapNone/><wp:docPr id="1" name="Star"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1143000" cy="762000"/></a:xfrm><a:prstGeom prst="star5"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill><a:effectLst><a:reflection blurRad="6350" stA="50000" endA="300" endPos="55000" dir="5400000" sy="-100000" algn="bl" rotWithShape="0"/></a:effectLst><a:scene3d><a:camera prst="orthographicFront"/><a:lightRig rig="threePt" dir="t"/></a:scene3d></wps:spPr><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing>"#;

    fn doc(alternate: bool) -> crate::docx::Document {
        match alternate {
            false => doc_of(&format!("<w:r>{DRAWING}</w:r>")),
            true => doc_of(&format!(
                r#"<w:r><mc:AlternateContent><mc:Choice Requires="wps">{DRAWING}</mc:Choice><mc:Fallback><w:pict><v:rect/></w:pict></mc:Fallback></mc:AlternateContent></w:r>"#
            )),
        }
    }

    fn doc_of(shape: &str) -> crate::docx::Document {
        doc_body(&format!("<w:p><w:r><w:t>Before</w:t></w:r>{shape}</w:p>"))
    }

    fn doc_body(body: &str) -> crate::docx::Document {
        let ct = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
        let rels = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let document = format!(
            r#"<?xml version="1.0"?><w:document {NS} xmlns:o="urn:schemas-microsoft-com:office:office"><w:body>{body}<w:sectPr/></w:body></w:document>"#
        );
        use std::io::Write;
        let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, xml) in [("[Content_Types].xml", ct), ("_rels/.rels", rels), ("word/document.xml", document.as_str())] {
            z.start_file(name, zip::write::SimpleFileOptions::default()).expect("a part");
            z.write_all(xml.as_bytes()).expect("written");
        }
        crate::docx::reader::read_editable(&z.finish().expect("a package").into_inner()).expect("reads")
    }

    fn document_xml(bytes: &[u8]) -> String {
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("a package");
        let mut s = String::new();
        z.by_name("word/document.xml").expect("the document part").read_to_string(&mut s).expect("text");
        s
    }

    fn refilled(alternate: bool) -> String {
        let mut d = doc(alternate);
        let Block::Para(p) = &mut d.sections[0].blocks[0] else { panic!() };
        let Some(Inline::Float(f, _)) = p.inlines.iter_mut().find(|i| matches!(i, Inline::Float(..))) else { panic!("a float") };
        f.shape.as_deref_mut().and_then(|s| s.sp_mut()).expect("a shape").fill = Some(Fill::Solid(Colour::srgb(0x00FF00)));
        document_xml(&crate::docx::write::save(&d).expect("saves"))
    }

    #[test]
    fn an_edited_shape_drops_its_fallback() {
        let original = document_xml(&crate::docx::write::save(&doc(true)).expect("saves"));
        assert!(original.contains("<mc:Fallback>"), "unedited: copied whole");
        let saved = refilled(true);
        let bare = document_xml(&crate::docx::write::save(&doc(false)).expect("saves"));
        assert_eq!(saved, bare.replace(r#"<a:srgbClr val="FF0000"/>"#, r#"<a:srgbClr val="00FF00"/>"#));
    }

    fn first_float(d: &mut crate::docx::Document) -> &mut crate::docx::model::Float {
        let Block::Para(p) = &mut d.sections[0].blocks[0] else { panic!() };
        p.inlines.iter_mut().find_map(|i| if let Inline::Float(f, _) = i { Some(&mut **f) } else { None }).expect("a float")
    }

    fn anchored(graphic: &str) -> String {
        DRAWING.replace(&DRAWING[DRAWING.find("<a:graphic>").expect("a graphic")..DRAWING.find("</wp:anchor>").expect("an anchor")], graphic)
    }

    #[test]
    fn a_floats_new_effect_extents_are_saved() {
        let mut d = doc_of(&format!("<w:r>{DRAWING}</w:r>"));
        first_float(&mut d).effect = [1.0, 2.0, 0.0, 0.0];
        let xml = document_xml(&crate::docx::write::save(&d).expect("saves"));
        assert!(xml.contains(r#"<wp:effectExtent l="12700" t="25400" r="0" b="0"/>"#), "{xml}");
    }

    #[test]
    fn a_pictures_new_outline_goes_in_its_properties() {
        let pic = r#"<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><pic:nvPicPr><pic:cNvPr id="1" name="p"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rId9"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1143000" cy="762000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic>"#;
        let mut d = doc_of(&format!("<w:r>{}</w:r>", anchored(pic)));
        let line = crate::drawing::paint::Line { fill: Some(Fill::Solid(Colour::srgb(0xFF0000))), ..Default::default() };
        let Some(crate::drawing::ShapeKind::Pic(p)) = first_float(&mut d).shape.as_deref_mut().map(|s| &mut s.kind) else { panic!("a picture") };
        p.line = Some(line.clone());
        let bytes = crate::docx::write::save(&d).expect("saves");
        let xml = document_xml(&bytes);
        assert!(xml.contains(r#"<a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:ln>"#), "{xml}");
        let mut again = crate::docx::reader::read_editable(&bytes).expect("reads");
        assert_eq!(first_float(&mut again).shape.as_deref().and_then(|s| s.pic()).and_then(|p| p.line.clone()), Some(line));
    }

    #[test]
    fn a_fill_goes_in_properties_whose_attribute_holds_a_greater_than() {
        let sp = r#"<a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:cNvSpPr/><wps:spPr xmlns:x="urn:x" x:label="a>b"/><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic>"#;
        let mut d = doc_of(&format!("<w:r>{}</w:r>", anchored(sp)));
        first_float(&mut d).shape.as_deref_mut().and_then(|s| s.sp_mut()).expect("a shape").fill = Some(Fill::Solid(Colour::srgb(0x00FF00)));
        let bytes = crate::docx::write::save(&d).expect("saves");
        let xml = document_xml(&bytes);
        assert!(xml.contains(r#"<wps:spPr xmlns:x="urn:x" x:label="a>b"><a:solidFill>"#), "{xml}");
        let mut again = crate::docx::reader::read_editable(&bytes).expect("reads");
        let fill = first_float(&mut again).shape.as_deref().and_then(|s| s.sp()).and_then(|s| s.fill.clone());
        assert_eq!(fill, Some(Fill::Solid(Colour::srgb(0x00FF00))));
    }

    #[test]
    fn a_changed_fill_replaces_only_the_fill_element() {
        let original = document_xml(&crate::docx::write::save(&doc(false)).expect("saves"));
        let saved = refilled(false);
        assert_eq!(saved, original.replace(r#"<a:srgbClr val="FF0000"/>"#, r#"<a:srgbClr val="00FF00"/>"#));
        assert!(saved.contains("<a:reflection") && saved.contains("<a:scene3d>"), "what isn't drawn is kept");
    }

    #[test]
    fn a_written_reflection_reads_as_it_was() {
        let xml = r#"<a:effectLst xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:reflection blurRad="6350" stA="50000" stPos="1000" endA="300" endPos="35000" dist="12700" dir="5400000" fadeDir="2700000" sx="90000" sy="-100000" kx="60000" ky="120000" algn="bl" rotWithShape="0"/></a:effectLst>"#;
        let read = |x: &str| crate::drawing::paint::EffectList::parse(roxmltree::Document::parse(x).expect("xml").root_element());
        let e = read(xml);
        assert!(e.reflection.is_some() && !e.kept);
        let written =
            super::effects_xml(&e).replace("<a:effectLst>", r#"<a:effectLst xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">"#);
        assert_eq!(read(&written), e);
    }

    #[test]
    fn new_shapes_save_as_they_were_made() {
        use crate::drawing::geom::Preset;
        use crate::drawing::units::Emu;
        use crate::drawing::{NewShape, Shape};
        let made = [
            Shape::new(NewShape::Shape(Preset::Star5), Emu(1143000), Emu(762000), "Star"),
            Shape::new(NewShape::Line(Preset::BentConnector3), Emu(1143000), Emu(762000), "Elbow Connector"),
        ];
        let mut d = doc(false);
        let Block::Para(p) = &mut d.sections[0].blocks[0] else { panic!() };
        let (f, run) =
            p.inlines.iter().find_map(|i| if let Inline::Float(f, r) = i { Some(((**f).clone(), r.clone())) } else { None }).expect("a float");
        for (k, shape) in made.iter().enumerate() {
            let g = crate::docx::model::Float { id: 1000 + k as u64, src: None, shape: Some(Box::new(shape.clone())), ..f.clone() };
            p.inlines.push(Inline::Float(Box::new(g), run.clone()));
        }
        let bytes = crate::docx::write::save(&d).expect("saves");
        let xml = document_xml(&bytes);
        assert!(xml.matches(r#"name="Star""#).count() == 2 && xml.contains(r#"name="Elbow Connector""#) && xml.contains("<wps:cNvCnPr/>"), "{xml}");
        let again = crate::docx::reader::read_editable(&bytes).expect("reads");
        let Block::Para(p) = &again.sections[0].blocks[0] else { panic!() };
        let read: Vec<_> = p.inlines.iter().filter_map(|i| if let Inline::Float(f, _) = i { f.shape.as_deref().cloned() } else { None }).collect();
        assert_eq!(read.len(), 3, "{xml}");
        for (shape, back) in made.iter().zip(&read) {
            assert_eq!(back.kind, shape.kind);
            assert_eq!(back.xfrm.ext, shape.xfrm.ext);
        }
    }

    fn vml_doc() -> crate::docx::Document {
        let ns = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office""#;
        let rect = r##"<w:r><w:pict><v:rect id="R" style="position:absolute;margin-left:72pt;margin-top:144pt;width:90pt;height:60pt;z-index:3;mso-position-horizontal-relative:page;mso-position-vertical-relative:page" fillcolor="#4472c4" strokecolor="black" strokeweight="1pt"/></w:pict></w:r>"##;
        let ct = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
        let rels = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let document =
            format!(r#"<?xml version="1.0"?><w:document {ns}><w:body><w:p><w:r><w:t>Text</w:t></w:r>{rect}</w:p><w:sectPr/></w:body></w:document>"#);
        use std::io::Write;
        let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, xml) in [("[Content_Types].xml", ct), ("_rels/.rels", rels), ("word/document.xml", document.as_str())] {
            z.start_file(name, zip::write::SimpleFileOptions::default()).expect("a part");
            z.write_all(xml.as_bytes()).expect("written");
        }
        crate::docx::reader::read_editable(&z.finish().expect("a package").into_inner()).expect("reads")
    }

    fn vml_float(d: &mut crate::docx::Document) -> &mut crate::docx::model::Float {
        let Block::Para(p) = &mut d.sections[0].blocks[0] else { panic!() };
        p.inlines.iter_mut().find_map(|i| if let Inline::Float(f, _) = i { Some(&mut **f) } else { None }).expect("the VML float")
    }

    #[test]
    fn vml_reads_as_a_float_and_saves_its_edits_as_vml() {
        let mut d = vml_doc();
        let f = vml_float(&mut d);
        assert_eq!((f.h_relative.as_str(), f.h_offset, f.v_offset, f.w, f.h, f.z), ("page", 72.0, 144.0, 90.0, 60.0, 3));
        assert!(matches!(f.shape.as_deref().and_then(|s| s.sp()).and_then(|s| s.fill.clone()), Some(Fill::Solid(c)) if c == Colour::srgb(0x4472C4)));
        let original = document_xml(&crate::docx::write::save(&vml_doc()).expect("saves"));
        assert!(original.contains(r##"fillcolor="#4472c4""##), "unedited VML is copied");
        f.h_offset = 100.0;
        f.shape.as_deref_mut().and_then(|s| s.sp_mut()).expect("a shape").fill = Some(Fill::Solid(Colour::srgb(0x00FF00)));
        let saved = document_xml(&crate::docx::write::save(&d).expect("saves"));
        assert!(saved.contains("margin-left:100pt") && saved.contains(r##"fillcolor="#00ff00""##), "{saved}");
        assert_eq!(
            saved
                .replace("margin-left:100pt", "margin-left:72pt")
                .replace(r##"fillcolor="#00ff00""##, r##"fillcolor="#4472c4""##)
                .replace(r#" filled="t""#, ""),
            original
        );
    }

    #[test]
    fn a_cleared_fill_or_line_width_isnt_taken_for_what_vml_says() {
        let mut d = vml_doc();
        let a = vml_float(&mut d).shape.as_deref().cloned().expect("a shape");
        let with = |f: &dyn Fn(&mut crate::drawing::SpShape)| {
            let mut b = a.clone();
            f(b.sp_mut().expect("a shape"));
            super::vml_says(&a, &b)
        };
        assert!(with(&|s| s.fill = Some(Fill::Solid(Colour::srgb(0x00FF00)))), "a new solid fill is written as VML");
        assert!(!with(&|s| s.fill = None), "a cleared fill");
        assert!(a.sp().and_then(|s| s.line.as_ref()).is_some_and(|l| l.width.is_some()));
        assert!(!with(&|s| s.line.as_mut().expect("a line").width = None), "a cleared width");
        assert!(with(&|s| s.line.as_mut().expect("a line").width = Some(crate::drawing::units::Emu(25400))), "a new width");
    }

    #[test]
    fn a_vml_shape_edited_past_vml_is_saved_as_drawingml() {
        use crate::drawing::paint::{EffectList, Glow};
        let mut d = vml_doc();
        let f = vml_float(&mut d);
        let s = f.shape.as_deref_mut().expect("a shape");
        s.nv.hidden = true;
        s.nv.title = "Box".into();
        let saved = document_xml(&crate::docx::write::save(&d).expect("saves"));
        assert!(saved.contains("visibility:hidden") && saved.contains(r#"title="Box""#) && saved.contains("<v:rect"), "VML says these: {saved}");
        let f = vml_float(&mut d);
        let glow = Glow { radius: crate::drawing::units::Emu(76200), colour: Colour::srgb(0xFF0000) };
        f.shape.as_deref_mut().and_then(|s| s.sp_mut()).expect("a shape").effects = Some(EffectList { glow: Some(glow), ..EffectList::default() });
        let saved = document_xml(&crate::docx::write::save(&d).expect("saves"));
        assert!(saved.contains("<a:glow") && saved.contains("<wp:anchor") && !saved.contains("<v:rect"), "VML has no glow: {saved}");
        let again = crate::docx::reader::read_editable(&crate::docx::write::save(&d).expect("saves")).expect("reads");
        let Block::Para(p) = &again.sections[0].blocks[0] else { panic!() };
        let back = p.inlines.iter().find_map(|i| if let Inline::Float(f, _) = i { f.shape.clone() } else { None }).expect("the float");
        assert!(back.nv.hidden && &*back.nv.title == "Box");
        assert!(back.sp().and_then(|s| s.effects.as_ref()).is_some_and(|e| e.glow.is_some()));
    }

    #[test]
    fn a_vml_pictures_adjustments_are_saved_on_its_imagedata() {
        use crate::drawing::paint::{BlipAdjust, BlipFill, BlipMode};
        use crate::drawing::units::Pct;
        let a = crate::drawing::paint::BlipFill {
            image: Some(0),
            src: [Pct(0); 4],
            mode: BlipMode::Stretch([Pct(0); 4]),
            adjust: Vec::new(),
            rot_with_shape: true,
        };
        let b = BlipFill {
            adjust: vec![BlipAdjust::Lum { bright: Pct(20000), contrast: Pct(-30000) }, BlipAdjust::Grayscale],
            src: [Pct(10000), Pct(0), Pct(0), Pct(0)],
            ..a.clone()
        };
        let tag = super::imagedata_patch(r#"<v:imagedata r:id="rId5" o:title=""/>"#, &b);
        assert!(
            tag.contains(r#"gain="45875f""#)
                && tag.contains(r#"blacklevel="6554f""#)
                && tag.contains(r#"grayscale="t""#)
                && tag.contains(r#"cropleft="6554f""#),
            "{tag}"
        );
        let back = super::imagedata_patch(&tag, &a);
        assert!(back.contains(r#"gain="65536f""#) && back.contains(r#"grayscale="f""#), "{back}");
    }

    const PIC: &str = r#"<a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><pic:nvPicPr><pic:cNvPr id="1" name="p"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rId9"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1143000" cy="762000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic>"#;

    fn parses(xml: &str) {
        if let Err(e) = roxmltree::Document::parse(xml) {
            panic!("{e}: {xml}");
        }
    }

    #[test]
    fn a_vml_pictures_new_outline_isnt_taken_for_what_vml_says() {
        let mut d = doc_of(&format!("<w:r>{}</w:r>", anchored(PIC)));
        let a = first_float(&mut d).shape.as_deref().cloned().expect("a picture");
        let mut b = a.clone();
        let crate::drawing::ShapeKind::Pic(p) = &mut b.kind else { panic!("a picture") };
        p.line = Some(crate::drawing::paint::Line { fill: Some(Fill::Solid(Colour::srgb(0xFF0000))), ..Default::default() });
        assert!(!super::vml_says(&a, &b), "neither VML patch writes a picture's outline");
    }

    #[test]
    fn a_renamed_drawing_replaces_a_single_quoted_name() {
        let mut d = doc_of(&format!("<w:r>{}</w:r>", DRAWING.replace(r#"name="Star""#, "name='Old' title='T'")));
        let s = first_float(&mut d).shape.as_deref_mut().expect("a shape");
        (s.nv.name, s.nv.title) = ("New".into(), "".into());
        let xml = document_xml(&crate::docx::write::save(&d).expect("saves"));
        parses(&xml);
        assert!(xml.contains(r#"name="New""#) && !xml.contains("'Old'") && !xml.contains("'T'"), "{xml}");
        // A group child's own name.
        let group = r#"<a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup"><wpg:wgp xmlns:wpg="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup"><wpg:cNvGrpSpPr/><wpg:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1143000" cy="762000"/><a:chOff x="0" y="0"/><a:chExt cx="1143000" cy="762000"/></a:xfrm></wpg:grpSpPr><wps:wsp><wps:cNvPr id="2" name='Old' hidden='0'/><wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1143000" cy="762000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></wps:spPr><wps:bodyPr/></wps:wsp></wpg:wgp></a:graphicData></a:graphic>"#;
        let mut d = doc_of(&format!("<w:r>{}</w:r>", anchored(group)));
        let c = first_float(&mut d).shape.as_deref_mut().and_then(|s| s.at_mut(&[0])).expect("the child");
        (c.nv.name, c.nv.hidden) = ("New".into(), true);
        let xml = document_xml(&crate::docx::write::save(&d).expect("saves"));
        parses(&xml);
        assert!(xml.contains(r#"<wps:cNvPr id="2" name="New" hidden="1"/>"#), "{xml}");
    }

    #[test]
    fn a_renamed_child_group_takes_its_own_name_not_its_childs() {
        let inner = r#"<wps:wsp><wps:cNvPr id="3" name="Inner"/><wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1143000" cy="762000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></wps:spPr><wps:bodyPr/></wps:wsp>"#;
        let xfrm =
            r#"<a:xfrm><a:off x="0" y="0"/><a:ext cx="1143000" cy="762000"/><a:chOff x="0" y="0"/><a:chExt cx="1143000" cy="762000"/></a:xfrm>"#;
        let wpg = "http://schemas.microsoft.com/office/word/2010/wordprocessingGroup";
        let group = format!(
            r#"<a:graphic><a:graphicData uri="{wpg}"><wpg:wgp xmlns:wpg="{wpg}"><wpg:cNvGrpSpPr/><wpg:grpSpPr>{xfrm}</wpg:grpSpPr><grpSp xmlns="{wpg}"><cNvPr id="2" name="Old"/><cNvGrpSpPr/><grpSpPr>{xfrm}</grpSpPr>{inner}</grpSp></wpg:wgp></a:graphicData></a:graphic>"#
        );
        let mut d = doc_of(&format!("<w:r>{}</w:r>", anchored(&group)));
        let c = first_float(&mut d).shape.as_deref_mut().and_then(|s| s.at_mut(&[0])).expect("the child group");
        assert_eq!(&*c.nv.name, "Old");
        c.nv.name = "New".into();
        let xml = document_xml(&crate::docx::write::save(&d).expect("saves"));
        parses(&xml);
        assert!(xml.contains(r#"<cNvPr id="2" name="New"/>"#) && xml.contains(r#"name="Inner""#), "{xml}");
    }

    #[test]
    fn a_new_groups_children_keep_their_titles() {
        let group = r#"<a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup"><wpg:wgp xmlns:wpg="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup"><wpg:cNvGrpSpPr/><wpg:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1143000" cy="762000"/><a:chOff x="0" y="0"/><a:chExt cx="1143000" cy="762000"/></a:xfrm></wpg:grpSpPr><wps:wsp><wps:cNvPr id="2" name="Box" title="A &amp; B"/><wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1143000" cy="762000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></wps:spPr><wps:bodyPr/></wps:wsp></wpg:wgp></a:graphicData></a:graphic>"#;
        let mut d = doc_of(&format!("<w:r>{}</w:r>", anchored(group)));
        let s = first_float(&mut d).shape.as_deref().cloned().expect("a group");
        let xml = super::graphic_data(&s, &|_| None, "w:");
        assert!(xml.contains(r#"<wps:cNvPr id="2" name="Box""#) && xml.contains(r#" title="A &amp; B"/>"#), "{xml}");
    }

    #[test]
    fn an_unlocked_group_keeps_its_pictures_locks() {
        let pic = PIC.replace("<pic:cNvPicPr/>", r#"<pic:cNvPicPr><a:picLocks noChangeAspect="1"/></pic:cNvPicPr>"#);
        let body = &pic[pic.find("<pic:pic ").expect("a picture")..pic.find("</a:graphicData>").expect("its end")];
        let xfrm =
            r#"<a:xfrm><a:off x="0" y="0"/><a:ext cx="1143000" cy="762000"/><a:chOff x="0" y="0"/><a:chExt cx="1143000" cy="762000"/></a:xfrm>"#;
        let wpg = "http://schemas.microsoft.com/office/word/2010/wordprocessingGroup";
        let group = format!(
            r#"<wp:cNvGraphicFramePr><a:graphicFrameLocks noChangeAspect="1"/></wp:cNvGraphicFramePr><a:graphic><a:graphicData uri="{wpg}"><wpg:wgp xmlns:wpg="{wpg}"><wpg:cNvGrpSpPr/><wpg:grpSpPr>{xfrm}</wpg:grpSpPr>{body}</wpg:wgp></a:graphicData></a:graphic>"#
        );
        let mut d = doc_of(&format!("<w:r>{}</w:r>", anchored(&group)));
        let f = first_float(&mut d);
        assert!(f.lock_aspect, "its frame's lock");
        f.lock_aspect = false;
        let bytes = crate::docx::write::save(&d).expect("saves");
        let xml = document_xml(&bytes);
        parses(&xml);
        assert!(xml.contains(r#"<a:graphicFrameLocks noChangeAspect="0"/>"#) && xml.contains(r#"<a:picLocks noChangeAspect="1"/>"#), "{xml}");
        let mut again = crate::docx::reader::read_editable(&bytes).expect("reads");
        assert!(!first_float(&mut again).lock_aspect, "read back unlocked");
    }

    #[test]
    fn a_vml_float_saved_as_drawingml_keeps_its_place_in_the_stack() {
        use crate::drawing::paint::{EffectList, Glow};
        let mut d = vml_doc();
        let f = vml_float(&mut d);
        let glow = Glow { radius: crate::drawing::units::Emu(76200), colour: Colour::srgb(0xFF0000) };
        f.shape.as_deref_mut().and_then(|s| s.sp_mut()).expect("a shape").effects = Some(EffectList { glow: Some(glow), ..EffectList::default() });
        let saved = document_xml(&crate::docx::write::save(&d).expect("saves"));
        assert!(saved.contains(r#"relativeHeight="3""#), "its z-index: {saved}");
    }

    #[test]
    fn a_linked_vml_text_box_is_saved_as_drawingml_with_its_chain() {
        let vml = r##"<w:r><w:pict><v:shape id="T" type="#_x0000_t202" style="position:absolute;margin-left:72pt;margin-top:72pt;width:90pt;height:60pt;z-index:2;mso-position-horizontal-relative:page;mso-position-vertical-relative:page"><v:textbox><w:txbxContent><w:p><w:r><w:t>Story</w:t></w:r></w:p></w:txbxContent></v:textbox></v:shape></w:pict></w:r>"##;
        let mut d = doc_of(vml);
        let sp = first_float(&mut d).shape.as_deref_mut().and_then(|s| s.sp_mut()).expect("a text box");
        assert_eq!(sp.text, crate::drawing::ShapeText::Story);
        sp.chain = Some(7);
        let xml = document_xml(&crate::docx::write::save(&d).expect("saves"));
        assert!(xml.contains(r#"<wps:txbx id="7">"#) && xml.contains("Story") && !xml.contains("<v:shape"), "{xml}");
    }

    #[test]
    fn a_box_moved_after_taking_a_story_of_its_own_saves_it() {
        let rect = r#"<a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:cNvSpPr txBox="1"/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1143000" cy="762000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></wps:spPr>TEXT<wps:bodyPr/></wps:wsp></a:graphicData></a:graphic>"#;
        let head =
            anchored(&rect.replace("TEXT", r#"<wps:txbx id="1"><w:txbxContent><w:p><w:r><w:t>Story</w:t></w:r></w:p></w:txbxContent></wps:txbx>"#));
        let next = anchored(&rect.replace("TEXT", r#"<wps:linkedTxbx id="1" seq="1"/>"#)).replace(r#"<wp:docPr id="1""#, r#"<wp:docPr id="2""#);
        let mut d =
            doc_body(&format!(r#"<w:p><w:r><w:t>One</w:t></w:r><w:r>{head}</w:r><w:r>{next}</w:r></w:p><w:p><w:r><w:t>Two</w:t></w:r></w:p>"#));
        let Block::Para(p) = &mut d.sections[0].blocks[0] else { panic!() };
        let floats: Vec<usize> = p.inlines.iter().enumerate().filter(|(_, i)| matches!(i, Inline::Float(..))).map(|(k, _)| k).collect();
        let moved = p.inlines.remove(floats[1]);
        let Inline::Float(head, _) = &mut p.inlines[floats[0]] else { panic!() };
        head.shape.as_deref_mut().and_then(|s| s.sp_mut()).expect("a box").chain = None;
        let mut story = head.text_box.clone().expect("the story");
        let Some(Block::Para(q)) = story.blocks.first_mut() else { panic!() };
        q.id = crate::docx::next_para_id();
        for i in &mut q.inlines {
            if let Inline::Text(t, _) = i {
                *t = "Moved".into();
            }
        }
        let Inline::Float(mut b, run) = moved else { panic!() };
        b.shape.as_deref_mut().and_then(|s| s.sp_mut()).expect("a box").text = crate::drawing::ShapeText::Story;
        b.text_box = Some(story);
        let Block::Para(two) = &mut d.sections[0].blocks[1] else { panic!() };
        two.inlines.push(Inline::Float(b, run));
        let bytes = crate::docx::write::save(&d).expect("saves");
        let xml = document_xml(&bytes);
        parses(&xml);
        assert!(xml.contains("Moved") && !xml.contains("linkedTxbx"), "{xml}");
        assert_eq!(xml.matches("<wps:txbx>").count(), 2, "neither box heads a chain: {xml}");
        let again = crate::docx::reader::read_editable(&bytes).expect("reads");
        let Block::Para(two) = &again.sections[0].blocks[1] else { panic!() };
        assert!(two.inlines.iter().any(|i| matches!(i, Inline::Float(f, _) if f.text_box.is_some())), "the moved box holds its story");
    }

    fn inline_pic(effect: &str) -> String {
        format!(
            r#"<w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="1143000" cy="762000"/>{effect}<wp:docPr id="3" name="Pic"/>{PIC}</wp:inline></w:drawing>"#
        )
    }

    fn first_picture(d: &mut crate::docx::Document) -> &mut crate::docx::model::Picture {
        let Block::Para(p) = &mut d.sections[0].blocks[0] else { panic!() };
        p.inlines.iter_mut().find_map(|i| if let Inline::Picture(p, _) = i { Some(p) } else { None }).expect("a picture")
    }

    #[test]
    fn a_resized_pictures_effect_extent_keeps_its_end_tag() {
        let mut d = doc_of(&format!("<w:r>{}</w:r>", inline_pic(r#"<wp:effectExtent l="0" t="0" r="0" b="0"></wp:effectExtent>"#)));
        let p = first_picture(&mut d);
        (p.w, p.effect) = (120.0, [1.0, 0.0, 0.0, 0.0]);
        let xml = document_xml(&crate::docx::write::save(&d).expect("saves"));
        parses(&xml);
        assert!(xml.contains(r#"<wp:effectExtent l="12700" t="0" r="0" b="0"></wp:effectExtent>"#), "{xml}");
    }

    #[test]
    fn a_new_effect_extent_goes_after_a_paired_extent() {
        let pic = inline_pic("").replace(r#"<wp:extent cx="1143000" cy="762000"/>"#, r#"<wp:extent cx="1143000" cy="762000"></wp:extent>"#);
        let mut d = doc_of(&format!("<w:r>{pic}</w:r>"));
        let p = first_picture(&mut d);
        (p.w, p.effect) = (120.0, [1.0, 0.0, 0.0, 0.0]);
        let xml = document_xml(&crate::docx::write::save(&d).expect("saves"));
        parses(&xml);
        assert!(xml.contains(r#"</wp:extent><wp:effectExtent l="12700" t="0" r="0" b="0"/>"#), "{xml}");
    }

    #[test]
    fn a_picture_controls_cropped_picture_saves_its_crop() {
        use crate::drawing::units::Pct;
        let sdt =
            format!(r#"<w:sdt><w:sdtPr><w:id w:val="5"/><w:picture/></w:sdtPr><w:sdtContent><w:r>{}</w:r></w:sdtContent></w:sdt>"#, inline_pic(""));
        let mut d = doc_of(&sdt);
        let p = first_picture(&mut d);
        p.w = 100.0;
        let Some(crate::drawing::ShapeKind::Pic(pic)) = p.shape.as_deref_mut().map(|s| &mut s.kind) else { panic!("a picture") };
        pic.blip.src = [Pct(10000), Pct(0), Pct(0), Pct(0)];
        let bytes = crate::docx::write::save(&d).expect("saves");
        let xml = document_xml(&bytes);
        parses(&xml);
        assert!(xml.contains("<w:picture/>") && xml.contains("<a:srcRect"), "{xml}");
        let mut again = crate::docx::reader::read_editable(&bytes).expect("reads");
        let src = first_picture(&mut again).shape.as_deref().and_then(|s| s.pic()).map(|p| p.blip.src);
        assert_eq!(src, Some([Pct(10000), Pct(0), Pct(0), Pct(0)]));
    }

    #[test]
    fn a_picture_controls_picture_keeps_its_rename_and_resize_when_restyled() {
        let sdt =
            format!(r#"<w:sdt><w:sdtPr><w:id w:val="5"/><w:picture/></w:sdtPr><w:sdtContent><w:r>{}</w:r></w:sdtContent></w:sdt>"#, inline_pic(""));
        let mut d = doc_of(&sdt);
        let p = first_picture(&mut d);
        p.shape.as_deref_mut().expect("a shape").nv.name = "New".into();
        let xml = document_xml(&crate::docx::write::save(&d).expect("saves"));
        assert!(xml.contains(r#"name="New""#), "{xml}");
        let Block::Para(para) = &mut d.sections[0].blocks[0] else { panic!() };
        let Some((p, run)) = para.inlines.iter_mut().find_map(|i| if let Inline::Picture(p, r) = i { Some((p, r)) } else { None }) else {
            panic!("a picture")
        };
        p.w = 100.0;
        let direct = crate::docx::model::Raw(Some("<w:rPr><w:b/></w:rPr>".into()));
        *run = std::rc::Rc::new(crate::docx::RunStyle { bold: true, direct, ..(**run).clone() });
        let xml = document_xml(&crate::docx::write::save(&d).expect("saves"));
        parses(&xml);
        assert!(xml.contains("<w:b/>") && xml.contains(r#"<wp:extent cx="1270000""#) && xml.contains(r#"name="New""#), "{xml}");
    }

    #[test]
    fn a_vml_pictures_aspect_lock_is_saved() {
        let vml = r#"<w:pict><v:shape id="P" style="width:90pt;height:60pt"><v:imagedata xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:id="rId9" o:title=""/></v:shape></w:pict>"#;
        let mut d = doc_of(&format!("<w:r>{vml}</w:r>"));
        let p = first_picture(&mut d);
        assert!(!p.lock_aspect);
        p.lock_aspect = true;
        let bytes = crate::docx::write::save(&d).expect("saves");
        parses(&document_xml(&bytes));
        let mut again = crate::docx::reader::read_editable(&bytes).expect("reads");
        assert!(first_picture(&mut again).lock_aspect, "{}", document_xml(&bytes));
    }

    #[test]
    fn a_vml_picture_in_a_control_saved_as_drawingml_keeps_its_control() {
        use crate::drawing::paint::{EffectList, Glow};
        let vml = r#"<w:pict><v:shape id="P" style="width:90pt;height:60pt"><v:imagedata xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:id="rId9" o:title=""/></v:shape></w:pict>"#;
        let sdt = format!(
            r#"<w:sdt><w:sdtPr><w:id w:val="5"/><w:picture/></w:sdtPr><w:sdtContent><w:r><w:rPr><w:b/></w:rPr>{vml}</w:r></w:sdtContent></w:sdt>"#
        );
        let mut d = doc_of(&sdt);
        let p = first_picture(&mut d);
        let glow = Glow { radius: crate::drawing::units::Emu(76200), colour: Colour::srgb(0xFF0000) };
        let Some(crate::drawing::ShapeKind::Pic(pic)) = p.shape.as_deref_mut().map(|s| &mut s.kind) else { panic!("a picture") };
        pic.effects = Some(EffectList { glow: Some(glow), ..EffectList::default() });
        let xml = document_xml(&crate::docx::write::save(&d).expect("saves"));
        parses(&xml);
        assert!(xml.contains(r#"<w:sdtPr><w:id w:val="5"/><w:picture/></w:sdtPr><w:sdtContent><w:r><w:rPr><w:b/></w:rPr><w:drawing>"#), "{xml}");
        assert!(xml.contains("<a:glow") && !xml.contains("<w:pict>"), "{xml}");
    }
}
