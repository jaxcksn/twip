//! Shape saving

use std::ops::Range;

use crate::docx::model::Float;
use crate::drawing::colour::{Colour, ColourBase, ColourMod};
use crate::drawing::geom::Geometry;
use crate::drawing::paint::{BlipFill, BlipMode, Cap, Compound, Dash, EffectList, Fill, GradientKind, GradientPath, Join, Line, LineEnd, TileFlip};
use crate::drawing::text::{AutoFit, BodyProps, VAnchor};
use crate::drawing::{Shape, ShapeKind, Spans};

use super::props::esc;

pub(super) type Edit = (Range<usize>, String);

pub(super) fn changed(was: &Float, now: &Float, text: &str) -> Option<Vec<Edit>> {
    let (Some(a), Some(b), Some(src)) = (was.shape.as_deref(), now.shape.as_deref(), was.src.as_ref()) else { return None };
    shape_changed(a, b, src.start, text)
}

pub(super) fn story_added(was: &Float, now: &Float) -> Option<usize> {
    use crate::drawing::ShapeText;
    let (a, b, src) = (was.shape.as_deref()?, now.shape.as_deref()?, was.src.as_ref()?);
    let added = was.text_box.is_none() && now.text_box.is_some() && a.sp()?.text == ShapeText::None && b.sp()?.text == ShapeText::Story;
    added.then(|| a.spans.body.as_ref().map(|r| r.start - src.start)).flatten()
}

pub(super) fn is_vml(text: &str) -> bool {
    !text.contains("<wp:") && (text.contains("<v:") || text.contains(":pict"))
}

pub(super) fn vml_patch(text: &str, was: &Float, now: &Float) -> String {
    use crate::drawing::paint::Fill as F;
    let Some((start, end)) = first_vml_shape(text) else { return text.to_string() };
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
            let f = match (b.xfrm.flip_h, b.xfrm.flip_v) {
                (true, true) => "x y",
                (true, false) => "x",
                (false, true) => "y",
                _ => "",
            };
            set("flip", f.to_string());
        }
    }
    if (was.z, was.behind) != (now.z, now.behind) {
        let z = i64::from(now.z.max(1)) * if now.behind { -1 } else { 1 };
        set("z-index", z.to_string());
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
    format!("{}{tag}{}", &text[..start], &text[end..])
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

fn attr(tag: &str, name: &str) -> Option<String> {
    let key = format!(" {name}=\"");
    let v0 = tag.find(&key)? + key.len();
    let v1 = v0 + tag[v0..].find('"')?;
    Some(tag[v0..v1].to_string())
}

fn set_attr(tag: &str, name: &str, value: &str) -> String {
    let key = format!(" {name}=\"");
    match tag.find(&key) {
        Some(k) => {
            let v0 = k + key.len();
            let v1 = tag[v0..].find('"').map_or(tag.len(), |e| v0 + e);
            format!("{}{}{}", &tag[..v0], esc(value), &tag[v1..])
        }
        None => {
            let at = if tag.ends_with("/>") { tag.len() - 2 } else { tag.len() - 1 };
            format!("{} {name}=\"{}\"{}", &tag[..at], esc(value), &tag[at..])
        }
    }
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

pub(super) fn apply(src: &str, mut edits: Vec<Edit>) -> String {
    edits.sort_by_key(|(r, _)| std::cmp::Reverse(r.start));
    let mut s = src.to_string();
    for (r, with) in edits {
        if r.end <= s.len() && r.start <= r.end {
            s.replace_range(r, &with);
        }
    }
    s
}

fn shape_edits(a: &Shape, b: &Shape, base: usize, text: &str, out: &mut Vec<Edit>) {
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
        if let Some(end) = text.get(r.start..).and_then(|t| t.find('>')).map(|i| r.start + i + 1) {
            let closed = text[..end].ends_with("/>");
            out.push((r.start..end, xfrm_tag(b, closed)));
        }
    }
    match (&a.kind, &b.kind) {
        (ShapeKind::Sp(x), ShapeKind::Sp(y)) => {
            if x.geom != y.geom
                && let Some(g) = &sp.geom
            {
                out.push((rel(g), geometry_xml(&y.geom)));
            }
            props_edits(sp, (&x.fill, &y.fill), (&x.line, &y.line), (&x.effects, &y.effects), &rel, out);
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
            props_edits(sp, (&None, &None), (&x.line, &y.line), (&x.effects, &y.effects), &rel, out);
        }
        (ShapeKind::Group(x), ShapeKind::Group(y)) if x.children.len() == y.children.len() => {
            for (ca, cb) in x.children.iter().zip(&y.children) {
                shape_edits(ca, cb, base, text, out);
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
    out: &mut Vec<Edit>,
) {
    let at = |r: &Option<Range<usize>>| r.as_ref().map(|r| rel(r).end);
    let start_of_props = sp.sp_pr.as_ref().map(|r| rel(r).start);
    let after = |rs: &[&Option<Range<usize>>]| rs.iter().find_map(|r| at(r)).or(sp.xfrm.as_ref().map(|r| rel(r).end));
    if fa != fb {
        let xml = fb.as_ref().map_or(String::new(), fill_xml);
        match &sp.fill {
            Some(r) => out.push((rel(r), xml)),
            None => {
                if let Some(p) = after(&[&sp.geom]).or(start_of_props) {
                    out.push((p..p, xml));
                }
            }
        }
    }
    if la != lb {
        let xml = lb.as_ref().map_or(String::new(), line_xml);
        match &sp.line {
            Some(r) => out.push((rel(r), xml)),
            None => {
                if let Some(p) = after(&[&sp.fill, &sp.geom]) {
                    out.push((p..p, xml));
                }
            }
        }
    }
    if ea != eb {
        let xml = eb.as_ref().map_or(String::new(), effects_xml);
        match &sp.effects {
            Some(r) => out.push((rel(r), xml)),
            None => {
                if let Some(p) = after(&[&sp.line, &sp.fill, &sp.geom]) {
                    out.push((p..p, xml));
                }
            }
        }
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
    format!(r#"<{tag} rotWithShape="{}"><a:blip{embed}/>{src}{mode}</{tag}>"#, u8::from(b.rot_with_shape))
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
    if let Some(r) = e.soft_edge {
        s.push_str(&format!(r#"<a:softEdge rad="{}"/>"#, r.0));
    }
    s.push_str("</a:effectLst>");
    s
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
    format!(
        r#"<wps:bodyPr rot="{}" vert="{}" wrap="{}" lIns="{}" tIns="{}" rIns="{}" bIns="{}" anchor="{anchor}" anchorCtr="0"{up}>{fit}</wps:bodyPr>"#,
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
            let text = match txbx(path).filter(|_| sp.text == crate::drawing::ShapeText::Story) {
                Some(t) => format!("<wps:txbx><{w}txbxContent>{t}</{w}txbxContent></wps:txbx>"),
                None => String::new(),
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
        let shape = match alternate {
            false => format!("<w:r>{DRAWING}</w:r>"),
            true => format!(
                r#"<w:r><mc:AlternateContent><mc:Choice Requires="wps">{DRAWING}</mc:Choice><mc:Fallback><w:pict><v:rect/></w:pict></mc:Fallback></mc:AlternateContent></w:r>"#
            ),
        };
        let ct = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
        let rels = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
        let document = format!(
            r#"<?xml version="1.0"?><w:document {NS}><w:body><w:p><w:r><w:t>Before</w:t></w:r>{shape}</w:p><w:sectPr/></w:body></w:document>"#
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

    #[test]
    fn a_changed_fill_replaces_only_the_fill_element() {
        let original = document_xml(&crate::docx::write::save(&doc(false)).expect("saves"));
        let saved = refilled(false);
        assert_eq!(saved, original.replace(r#"<a:srgbClr val="FF0000"/>"#, r#"<a:srgbClr val="00FF00"/>"#));
        assert!(saved.contains("<a:reflection") && saved.contains("<a:scene3d>"), "what isn't drawn is kept");
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
        assert!(xml.contains(r#"name="Star "#) && xml.contains("<wps:cNvCnPr/>"), "{xml}");
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
}
