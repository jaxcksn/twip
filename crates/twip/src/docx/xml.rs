//! Thin helpers over roxmltree for WordprocessingML: `w:`-namespaced children and attributes, on/off values, lengths.

use roxmltree::Node;

pub const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
pub const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
pub const WP: &str = "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing";
pub const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
pub const W14: &str = "http://schemas.microsoft.com/office/word/2010/wordml";
pub const W15: &str = "http://schemas.microsoft.com/office/word/2012/wordml";
pub const MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";

pub fn is(n: Node, ns: &str, name: &str) -> bool {
    n.is_element() && n.tag_name().name() == name && n.tag_name().namespace() == Some(ns)
}

pub fn is_w(n: Node, name: &str) -> bool {
    is(n, W, name)
}

pub fn child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| is_w(*c, name))
}

pub fn child_ns<'a, 'i>(n: Node<'a, 'i>, ns: &str, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| is(*c, ns, name))
}

pub fn children<'a, 'i>(n: Node<'a, 'i>, name: &'static str) -> impl Iterator<Item = Node<'a, 'i>> {
    n.children().filter(move |c| is_w(*c, name))
}

/// A `w:` attribute (Word also accepts unqualified ones in some generators).
pub fn attr<'a>(n: Node<'a, '_>, name: &str) -> Option<&'a str> {
    n.attribute((W, name)).or_else(|| n.attribute(name))
}

pub fn val<'a>(n: Node<'a, '_>) -> Option<&'a str> {
    attr(n, "val")
}

/// ST_OnOff: absent `w:val` is on.
pub fn on_off_str(v: Option<&str>) -> bool {
    !matches!(v, Some("0" | "false" | "off"))
}

pub fn on_off(n: Node) -> bool {
    on_off_str(val(n))
}

/// An on/off child element: None when absent.
pub fn flag(n: Node, name: &str) -> Option<bool> {
    child(n, name).map(on_off)
}

pub fn int(s: Option<&str>) -> Option<i64> {
    let s = s?.trim();
    s.parse::<i64>().ok().or_else(|| s.parse::<f64>().ok().map(|v| v.round() as i64))
}

pub fn num(s: Option<&str>) -> Option<f64> {
    s?.trim().parse::<f64>().ok()
}

/// ST_TwipsMeasure / ST_SignedTwipsMeasure in points: twips, or a universal measure ("1in", "12pt", "2.5cm").
pub fn twips(s: Option<&str>) -> Option<f64> {
    let s = s?.trim();
    if let Ok(v) = s.parse::<f64>() {
        return Some(v / 20.0);
    }
    measure(s)
}

pub fn measure(s: &str) -> Option<f64> {
    if s.len() < 3 {
        return None;
    }
    let (v, unit) = s.split_at(s.len() - 2);
    let v: f64 = v.parse().ok()?;
    Some(match unit {
        "pt" => v,
        "in" => v * 72.0,
        "cm" => v * 72.0 / 2.54,
        "mm" => v * 72.0 / 25.4,
        "pc" | "pi" => v * 12.0,
        _ => return None,
    })
}

pub fn attr_twips(n: Node, name: &str) -> Option<f64> {
    twips(attr(n, name))
}

/// Half-points to points.
pub fn half_points(s: Option<&str>) -> Option<f64> {
    num(s).map(|v| v / 2.0).or_else(|| s.and_then(measure))
}

/// EMU to points.
pub fn emu(s: Option<&str>) -> Option<f64> {
    num(s).map(|v| v / 12700.0)
}

/// "RRGGBB" (or "auto" → None).
pub fn color(s: Option<&str>) -> Option<u32> {
    let s = s?.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    u32::from_str_radix(s, 16).ok()
}
