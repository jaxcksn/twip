//! Reading a drawing's graphic (`a:graphic` of a `wp:inline` or `wp:anchor`) into a [`Shape`] tree. Nothing here
//! fails: a graphic the engine does not draw becomes a raw graphic, kept on save.

use std::rc::Rc;

use roxmltree::Node;

use super::geom::{CustomGeom, Geometry, Preset};
use super::paint::{BlipFill, EffectList, Fill, Line, StyleRefs};
use super::text::BodyProps;
use super::units::{Angle, Emu};
use super::{GroupShape, NonVisual, PicShape, RawGraphic, Shape, ShapeKind, ShapeText, SpShape, Spans, Xfrm};
use crate::docx::xml::{A, R, W, WP};

/// What the reader of the document supplies: images by relationship id, and a SmartArt diagram's stored drawing by
/// the diagram's data relationship id (`dgm:relIds/@r:dm`).
pub trait Source {
    fn image(&mut self, rid: &str) -> Option<usize>;
    fn diagram(&mut self, dm: &str) -> Option<Shape>;
}

/// A reading in progress: the document's source, and the preset names it has no definition for (drawn as `rect`).
pub struct Ctx<'c> {
    pub source: &'c mut dyn Source,
    pub unknown_presets: Vec<String>,
}

fn named<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name)
}

fn a<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name && c.tag_name().namespace() == Some(A))
}

fn on(v: Option<&str>) -> bool {
    matches!(v, Some("1" | "true" | "on"))
}

fn xfrm_of(x: Option<Node>) -> (Xfrm, [Emu; 2], [Emu; 2]) {
    let Some(x) = x else { return (Xfrm::default(), [Emu(0); 2], [Emu(0); 2]) };
    let pair = |name: &str, k: [&str; 2]| a(x, name).map(|e| k.map(|k| e.attribute(k).and_then(Emu::parse).unwrap_or_default()));
    let ext = pair("ext", ["cx", "cy"]).unwrap_or_default();
    let f = Xfrm {
        off: pair("off", ["x", "y"]).unwrap_or_default(),
        ext,
        rot: x.attribute("rot").and_then(Angle::parse).unwrap_or_default(),
        flip_h: on(x.attribute("flipH")),
        flip_v: on(x.attribute("flipV")),
    };
    (f, pair("chOff", ["x", "y"]).unwrap_or_default(), pair("chExt", ["cx", "cy"]).unwrap_or(ext))
}

fn nv_of(c: Option<Node>) -> NonVisual {
    let Some(c) = c else { return NonVisual::default() };
    let descr = c.attribute("descr").filter(|s| !s.is_empty()).or(c.attribute("title")).unwrap_or("");
    NonVisual {
        id: c.attribute("id").and_then(|v| v.parse().ok()).unwrap_or(0),
        name: c.attribute("name").unwrap_or("").to_string(),
        descr: Rc::from(descr),
        hidden: on(c.attribute("hidden")),
        lock_aspect: false,
    }
}

fn geometry(sp_pr: Option<Node>, ctx: &mut Ctx) -> Geometry {
    let Some(p) = sp_pr else { return Geometry::default() };
    if let Some(g) = a(p, "prstGeom") {
        let name = g.attribute("prst").unwrap_or("rect");
        let adj = a(g, "avLst")
            .into_iter()
            .flat_map(|l| l.children().filter(|c| c.is_element()))
            .filter_map(|gd| {
                let v = gd.attribute("fmla")?.trim().strip_prefix("val ")?.trim().parse::<i64>().ok()?;
                Some((gd.attribute("name")?.to_string(), v))
            })
            .collect();
        let preset = Preset::from_name(name).unwrap_or_else(|| {
            ctx.unknown_presets.push(name.to_string());
            Preset::Rect
        });
        return Geometry::Preset { preset, adj };
    }
    if let Some(g) = a(p, "custGeom") {
        return Geometry::Custom(Rc::new(CustomGeom::parse(g)));
    }
    Geometry::default()
}

/// The fill, line and effects of a shape properties element, and whether it holds 3D.
struct Props {
    fill: Option<Fill>,
    line: Option<Line>,
    effects: Option<EffectList>,
    kept_3d: bool,
}

fn props(sp_pr: Option<Node>, ctx: &mut Ctx, spans: &mut Spans) -> Props {
    let Some(p) = sp_pr else { return Props { fill: None, line: None, effects: None, kept_3d: false } };
    let fill_el = p.children().filter(|c| c.is_element()).find(|c| {
        c.tag_name().namespace() == Some(A)
            && matches!(c.tag_name().name(), "noFill" | "solidFill" | "gradFill" | "pattFill" | "blipFill" | "grpFill")
    });
    spans.fill = fill_el.map(|n| n.range());
    let fill = fill_el.and_then(|f| Fill::parse_with(f, &mut |r| ctx.source.image(r)));
    let ln = a(p, "ln");
    spans.line = ln.map(|n| n.range());
    let line = ln.map(|l| Line::parse_with(l, &mut |r| ctx.source.image(r)));
    let eff = a(p, "effectLst");
    spans.effects = eff.or_else(|| a(p, "effectDag")).map(|n| n.range());
    let effects = eff.map(EffectList::parse);
    let kept_3d = a(p, "scene3d").is_some() || a(p, "sp3d").is_some() || a(p, "effectDag").is_some();
    Props { fill, line, effects, kept_3d }
}

fn sp(n: Node, ctx: &mut Ctx) -> Shape {
    let mut spans = Spans { shape: Some(n.range()), ..Spans::default() };
    let sp_pr = named(n, "spPr");
    spans.sp_pr = sp_pr.map(|p| p.range());
    let x = sp_pr.and_then(|p| a(p, "xfrm"));
    spans.xfrm = x.map(|e| e.range());
    spans.geom = sp_pr.and_then(|p| a(p, "prstGeom").or_else(|| a(p, "custGeom"))).map(|e| e.range());
    let (xfrm, ..) = xfrm_of(x);
    let geom = geometry(sp_pr, ctx);
    let Props { fill, line, effects, kept_3d } = props(sp_pr, ctx, &mut spans);
    let style = named(n, "style");
    spans.style = style.map(|s| s.range());
    let body = named(n, "bodyPr");
    spans.body = body.map(|b| b.range());
    let chain = named(n, "txbx").and_then(|t| t.attribute("id")).and_then(|v| v.parse().ok());
    let text = if let Some(t) = named(n, "txbx") {
        if t.children().any(|c| c.is_element() && c.tag_name().name() == "txbxContent" && c.tag_name().namespace() == Some(W)) {
            ShapeText::Story
        } else {
            ShapeText::None
        }
    } else if let Some(l) = named(n, "linkedTxbx") {
        let num = |k: &str| l.attribute(k).and_then(|v| v.parse().ok()).unwrap_or(0);
        ShapeText::Linked { chain: num("id"), seq: num("seq") }
    } else {
        ShapeText::None
    };
    let nv_sp = named(n, "cNvSpPr");
    Shape {
        nv: nv_of(named(n, "cNvPr")),
        xfrm,
        kind: ShapeKind::Sp(Box::new(SpShape {
            geom,
            fill,
            line,
            effects,
            style: style.map(StyleRefs::parse),
            body: body.map(BodyProps::parse).unwrap_or_default(),
            text,
            chain,
            text_box: nv_sp.is_some_and(|c| on(c.attribute("txBox"))),
            connector: named(n, "cNvCnPr").is_some(),
            kept_3d,
        })),
        spans,
    }
}

fn pic(n: Node, ctx: &mut Ctx) -> Shape {
    let mut spans = Spans { shape: Some(n.range()), ..Spans::default() };
    let sp_pr = named(n, "spPr");
    spans.sp_pr = sp_pr.map(|p| p.range());
    let x = sp_pr.and_then(|p| a(p, "xfrm"));
    spans.xfrm = x.map(|e| e.range());
    let (xfrm, ..) = xfrm_of(x);
    let geom = geometry(sp_pr, ctx);
    let Props { line, effects, kept_3d, .. } = props(sp_pr, ctx, &mut spans);
    let blip_fill = named(n, "blipFill");
    spans.fill = blip_fill.map(|b| b.range());
    let blip = blip_fill.map_or_else(|| BlipFill::parse(n, &mut |_| None), |b| BlipFill::parse(b, &mut |r| ctx.source.image(r)));
    let nv = named(n, "nvPicPr");
    let mut nonvisual = nv_of(nv.and_then(|v| named(v, "cNvPr")));
    nonvisual.lock_aspect =
        nv.and_then(|v| named(v, "cNvPicPr")).and_then(|c| named(c, "picLocks")).is_some_and(|l| on(l.attribute("noChangeAspect")));
    Shape { nv: nonvisual, xfrm, kind: ShapeKind::Pic(Box::new(PicShape { geom, blip, line, effects, kept_3d })), spans }
}

fn group(n: Node, canvas: bool, ctx: &mut Ctx) -> Shape {
    let gp = named(n, "grpSpPr").or_else(|| named(n, "bg"));
    let x = gp.and_then(|p| a(p, "xfrm"));
    let (xfrm, child_off, child_ext) = xfrm_of(x);
    let fill = gp.and_then(|p| Fill::child_of(p, &mut |r| ctx.source.image(r)));
    let children = n.children().filter(|c| c.is_element()).filter_map(|c| child(c, ctx)).collect::<Vec<_>>();
    let spans = Spans { shape: Some(n.range()), sp_pr: gp.map(|p| p.range()), xfrm: x.map(|e| e.range()), ..Spans::default() };
    let nv = nv_of(named(n, "cNvPr"));
    if children.is_empty() {
        return Shape { nv, xfrm, kind: ShapeKind::Raw(RawGraphic { uri: "group".into() }), spans };
    }
    Shape { nv, xfrm, kind: ShapeKind::Group(GroupShape { child_off, child_ext, fill, children, canvas }), spans }
}

/// A child of a group or canvas: a shape, picture, nested group or graphic frame.
fn child(c: Node, ctx: &mut Ctx) -> Option<Shape> {
    Some(match c.tag_name().name() {
        "wsp" | "sp" | "cxnSp" => sp(c, ctx),
        "pic" => pic(c, ctx),
        "grpSp" => group(c, false, ctx),
        "graphicFrame" => {
            let x = named(c, "xfrm");
            let (xfrm, ..) = xfrm_of(x);
            Shape {
                nv: nv_of(named(c, "cNvPr")),
                xfrm,
                kind: ShapeKind::Raw(RawGraphic { uri: "graphicFrame".into() }),
                spans: Spans { shape: Some(c.range()), ..Spans::default() },
            }
        }
        _ => return None,
    })
}

/// The shape of a `wp:inline` or `wp:anchor` (`item`): its graphic read into a tree, its extent from `wp:extent`,
/// its non-visual properties from `wp:docPr`.
pub fn read(item: Node, ctx: &mut Ctx) -> Shape {
    let extent = item.children().find(|c| c.is_element() && c.tag_name().name() == "extent" && c.tag_name().namespace() == Some(WP));
    let ext = ["cx", "cy"].map(|k| extent.and_then(|e| e.attribute(k)).and_then(Emu::parse).unwrap_or_default());
    let doc_pr = item.children().find(|c| c.is_element() && c.tag_name().name() == "docPr");
    let lock_aspect =
        item.descendants().any(|n| matches!(n.tag_name().name(), "graphicFrameLocks" | "picLocks") && on(n.attribute("noChangeAspect")));
    let data = item.descendants().find(|n| n.is_element() && n.tag_name().name() == "graphicData" && n.tag_name().namespace() == Some(A));
    let uri = data.and_then(|d| d.attribute("uri")).unwrap_or("").to_string();
    let body = data.and_then(|d| d.children().find(|c| c.is_element()));
    let mut s = match body {
        Some(b) => match b.tag_name().name() {
            "wsp" => sp(b, ctx),
            "pic" => pic(b, ctx),
            "wgp" => group(b, false, ctx),
            "wpc" => group(b, true, ctx),
            "relIds" => {
                let dm = b.attribute((R, "dm")).unwrap_or("");
                ctx.source.diagram(dm).unwrap_or_else(|| raw(&uri, b))
            }
            _ => raw(&uri, b),
        },
        None => raw(&uri, item),
    };
    // The anchor or inline gives the top-level shape's size and identity; its own offset is not used (FLOAT-15).
    s.xfrm.ext = ext;
    s.xfrm.off = [Emu(0); 2];
    let mut nv = nv_of(doc_pr);
    nv.lock_aspect = lock_aspect || s.nv.lock_aspect;
    s.nv = nv;
    s
}

fn raw(uri: &str, n: Node) -> Shape {
    Shape {
        nv: NonVisual::default(),
        xfrm: Xfrm::default(),
        kind: ShapeKind::Raw(RawGraphic { uri: uri.to_string() }),
        spans: Spans { shape: Some(n.range()), ..Spans::default() },
    }
}

/// A SmartArt diagram's stored drawing (`dsp:drawing`, the part a diagram's data names in `dsp:dataModelExt`): its
/// shapes as a group the diagram's size.
pub fn diagram_drawing(xml: &str, ctx: &mut Ctx) -> Option<Shape> {
    let doc = roxmltree::Document::parse(xml).ok()?;
    let tree = doc.descendants().find(|n| n.is_element() && n.tag_name().name() == "spTree")?;
    let children: Vec<Shape> = tree
        .children()
        .filter(|c| c.is_element())
        .filter_map(|c| child(c, ctx))
        .map(|mut s| {
            // The drawing part is not the document part: its spans mean nothing to saving.
            s.spans = Spans::default();
            s
        })
        .collect();
    if children.is_empty() {
        return None;
    }
    // The children are in EMU from the diagram's top left: the child frame is their bounding box from the origin.
    let (mut cx, mut cy) = (Emu(0), Emu(0));
    for c in &children {
        cx = cx.max(c.xfrm.off[0] + c.xfrm.ext[0]);
        cy = cy.max(c.xfrm.off[1] + c.xfrm.ext[1]);
    }
    Some(Shape {
        nv: NonVisual::default(),
        xfrm: Xfrm::default(),
        kind: ShapeKind::Group(GroupShape { child_off: [Emu(0); 2], child_ext: [cx, cy], fill: None, children, canvas: true }),
        spans: Spans::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawing::paint::Fill;

    const NS: &str = r#"xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape" xmlns:wpg="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main""#;

    struct Images;

    impl Source for Images {
        fn image(&mut self, rid: &str) -> Option<usize> {
            (rid == "rId5").then_some(3)
        }

        fn diagram(&mut self, _: &str) -> Option<Shape> {
            None
        }
    }

    fn read_xml(graphic: &str) -> (Shape, Vec<String>) {
        let xml = format!(
            r#"<wp:anchor {NS}><wp:extent cx="1270000" cy="635000"/><wp:docPr id="7" name="S" descr="alt"/><a:graphic><a:graphicData uri="x">{graphic}</a:graphicData></a:graphic></wp:anchor>"#
        );
        let doc = roxmltree::Document::parse(&xml).unwrap();
        let mut ctx = Ctx { source: &mut Images, unknown_presets: Vec::new() };
        let s = read(doc.root_element(), &mut ctx);
        (s, ctx.unknown_presets)
    }

    #[test]
    fn a_shape_without_text_is_read_with_its_paint() {
        let (s, unknown) = read_xml(
            r#"<wps:wsp><wps:cNvSpPr/><wps:spPr><a:xfrm rot="1800000" flipH="1"><a:off x="0" y="0"/><a:ext cx="1270000" cy="635000"/></a:xfrm><a:prstGeom prst="star5"><a:avLst><a:gd name="adj" fmla="val 10000"/></a:avLst></a:prstGeom><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill><a:ln w="12700"/><a:scene3d/></wps:spPr><wps:bodyPr/></wps:wsp>"#,
        );
        assert!(unknown.is_empty());
        assert_eq!((s.nv.id, &*s.nv.descr), (7, "alt"));
        assert_eq!(s.xfrm.ext, [Emu(1270000), Emu(635000)]);
        assert_eq!((s.xfrm.rot, s.xfrm.flip_h), (Angle(1800000), true));
        let sp = s.sp().unwrap();
        assert_eq!(sp.geom, Geometry::Preset { preset: Preset::from_name("star5").unwrap_or(Preset::Rect), adj: vec![("adj".into(), 10000)] });
        assert!(matches!(sp.fill, Some(Fill::Solid(_))));
        assert_eq!(sp.line.as_ref().and_then(|l| l.width), Some(Emu(12700)));
        assert!(sp.kept_3d);
        assert!(s.spans.shape.is_some() && s.spans.fill.is_some() && s.spans.line.is_some());
    }

    #[test]
    fn groups_pictures_and_unknown_graphics_are_read() {
        let (g, _) = read_xml(
            r#"<wpg:wgp><wpg:cNvGrpSpPr/><wpg:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="100" cy="100"/><a:chOff x="0" y="0"/><a:chExt cx="50" cy="50"/></a:xfrm></wpg:grpSpPr>
               <pic:pic><pic:nvPicPr><pic:cNvPr id="1" name="p"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rId5"/><a:srcRect l="10000"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="50" cy="50"/></a:xfrm><a:prstGeom prst="ellipse"><a:avLst/></a:prstGeom></pic:spPr></pic:pic>
               <wpg:grpSp><wpg:cNvPr id="2" name="g"/><wpg:cNvGrpSpPr/><wpg:grpSpPr/></wpg:grpSp></wpg:wgp>"#,
        );
        let gr = g.group().unwrap();
        assert_eq!((gr.child_ext, gr.children.len()), ([Emu(50), Emu(50)], 2));
        let p = gr.children[0].pic().unwrap();
        assert_eq!(p.blip.image, Some(3));
        assert!(matches!(gr.children[1].kind, ShapeKind::Raw(_)));
        let (r, _) = read_xml(r#"<c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" r:id="rId9"/>"#);
        assert!(matches!(r.kind, ShapeKind::Raw(_)));
        let (u, unknown) = read_xml(r#"<wps:wsp><wps:cNvSpPr/><wps:spPr><a:prstGeom prst="notAShape"/></wps:spPr><wps:bodyPr/></wps:wsp>"#);
        assert_eq!(u.sp().unwrap().geom, Geometry::default());
        assert_eq!(unknown, vec!["notAShape".to_string()]);
    }
}
