//! Custom geometry reading

use roxmltree::Node;

use crate::drawing::geom::{Arg, Cmd, CustomGeom, CxnDef, Formula, HandleDef, Op, PathFill, Pt, Var};

struct Scope {
    av: Vec<String>,
    gd: Vec<String>,
}

impl Scope {
    fn arg(&self, s: &str) -> Arg {
        if let Some(i) = self.gd.iter().rposition(|n| n == s) {
            return Arg::Guide(i as u16);
        }
        if let Some(i) = self.av.iter().position(|n| n == s) {
            return Arg::Adj(i as u8);
        }
        if let Some(a) = Var::parse(s) {
            return a;
        }
        let v = s.trim().parse::<i64>().or_else(|_| s.trim().parse::<f64>().map(|v| v.round() as i64)).unwrap_or(0);
        Arg::Const(v.clamp(i32::MIN.into(), i32::MAX.into()) as i32)
    }

    fn opt(&self, s: Option<&str>) -> Option<Arg> {
        s.map(|s| self.arg(s))
    }

    fn adj(&self, s: Option<&str>) -> Option<u8> {
        let s = s?;
        self.av.iter().position(|n| n == s).map(|i| i as u8)
    }
}

fn el<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name)
}

fn els<'a, 'i>(n: Node<'a, 'i>, name: &'static str) -> impl Iterator<Item = Node<'a, 'i>> {
    n.children().filter(move |c| c.is_element() && c.tag_name().name() == name)
}

fn pt(scope: &Scope, n: Option<Node>) -> Pt {
    let n = n.and_then(|n| if n.tag_name().name() == "pt" || n.tag_name().name() == "pos" { Some(n) } else { el(n, "pt") });
    [scope.arg(n.and_then(|p| p.attribute("x")).unwrap_or("0")), scope.arg(n.and_then(|p| p.attribute("y")).unwrap_or("0"))]
}

/// An `a:custGeom` (or a preset's definition, written as one), its guides, handles and paths compiled.
pub fn custom_geom(n: Node) -> CustomGeom {
    let mut scope = Scope { av: Vec::new(), gd: Vec::new() };
    let mut av = Vec::new();
    for gd in el(n, "avLst").into_iter().flat_map(|l| els(l, "gd")) {
        let v = gd.attribute("fmla").and_then(|f| f.trim().strip_prefix("val ")).and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0);
        scope.av.push(gd.attribute("name").unwrap_or("").to_string());
        av.push(v);
    }
    let mut ops = Vec::new();
    for gd in el(n, "gdLst").into_iter().flat_map(|l| els(l, "gd")) {
        let fmla = gd.attribute("fmla").unwrap_or("val 0");
        let mut parts = fmla.split_whitespace();
        let f = parts.next().and_then(Formula::parse).unwrap_or(Formula::Val);
        let args: Vec<Arg> = parts.map(|p| scope.arg(p)).collect();
        let a = |i: usize| args.get(i).copied().unwrap_or(Arg::Const(0));
        ops.push(Op { f, a: [a(0), a(1), a(2)] });
        scope.gd.push(gd.attribute("name").unwrap_or("").to_string());
    }
    let lim = |h: Node, r: &str, lo: &str, hi: &str| scope.adj(h.attribute(r)).map(|i| (i, scope.opt(h.attribute(lo)), scope.opt(h.attribute(hi))));
    let ah = el(n, "ahLst")
        .into_iter()
        .flat_map(|l| l.children().filter(Node::is_element))
        .filter_map(|h| match h.tag_name().name() {
            "ahXY" => Some(HandleDef::Xy { x: lim(h, "gdRefX", "minX", "maxX"), y: lim(h, "gdRefY", "minY", "maxY"), pos: pt(&scope, el(h, "pos")) }),
            "ahPolar" => Some(HandleDef::Polar {
                r: lim(h, "gdRefR", "minR", "maxR"),
                ang: lim(h, "gdRefAng", "minAng", "maxAng"),
                pos: pt(&scope, el(h, "pos")),
            }),
            _ => None,
        })
        .collect();
    let cxn = el(n, "cxnLst")
        .into_iter()
        .flat_map(|l| els(l, "cxn"))
        .map(|c| CxnDef { ang: scope.arg(c.attribute("ang").unwrap_or("0")), pos: pt(&scope, el(c, "pos")) })
        .collect();
    let rect = el(n, "rect").map(|r| ["l", "t", "r", "b"].map(|k| scope.arg(r.attribute(k).unwrap_or("0"))));
    let paths = el(n, "pathLst")
        .into_iter()
        .flat_map(|l| els(l, "path"))
        .map(|p| {
            let num = |k: &str| p.attribute(k).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
            let fill = match p.attribute("fill") {
                Some("none") => PathFill::None,
                Some("lighten") => PathFill::Lighten,
                Some("lightenLess") => PathFill::LightenLess,
                Some("darken") => PathFill::Darken,
                Some("darkenLess") => PathFill::DarkenLess,
                _ => PathFill::Norm,
            };
            let stroke = !matches!(p.attribute("stroke"), Some("0" | "false"));
            let mut cmds = Vec::new();
            let mut args = Vec::new();
            for c in p.children().filter(Node::is_element) {
                let cmd = match c.tag_name().name() {
                    "moveTo" => Cmd::MoveTo,
                    "lnTo" => Cmd::LineTo,
                    "arcTo" => Cmd::ArcTo,
                    "quadBezTo" => Cmd::QuadTo,
                    "cubicBezTo" => Cmd::CubicTo,
                    "close" => Cmd::Close,
                    _ => continue,
                };
                match cmd {
                    Cmd::ArcTo => args.extend(["wR", "hR", "stAng", "swAng"].map(|k| scope.arg(c.attribute(k).unwrap_or("0")))),
                    Cmd::Close => {}
                    _ => {
                        let mut qs = els(c, "pt");
                        for _ in 0..cmd.arity() / 2 {
                            args.extend(pt(&scope, qs.next()));
                        }
                    }
                }
                cmds.push(cmd);
            }
            (num("w"), num("h"), fill, stroke, cmds, args)
        })
        .collect();
    CustomGeom { av_names: scope.av, av, gd: ops, ah, cxn, rect, paths }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawing::geom::Geometry;
    use crate::drawing::units::Emu;

    #[test]
    fn a_sweep_from_a_guide_is_bounded_as_a_literal_one_is() {
        let xml = r#"<a:custGeom xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:avLst/><a:gdLst><a:gd name="g" fmla="*/ 2147483647 2147483647 1"/></a:gdLst><a:pathLst><a:path w="100" h="100"><a:moveTo><a:pt x="0" y="50"/></a:moveTo><a:arcTo wR="50" hR="50" stAng="0" swAng="g"/></a:path></a:pathLst></a:custGeom>"#;
        let d = roxmltree::Document::parse(xml).expect("xml");
        let g = Geometry::Custom(std::rc::Rc::new(custom_geom(d.root_element()))).evaluate(Emu::from_pt(100.0), Emu::from_pt(100.0));
        let segs: usize = g.paths.iter().map(|p| p.segs.len()).sum();
        assert!(segs < 1000, "{segs} segments");
    }
}
