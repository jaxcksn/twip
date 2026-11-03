//! DrawingML shapes laid out: every shape of a float drawn as a shape item at its mapped place (FLOAT-15, DRAW-4), with
//! its rotation and flips, in paint order (FLOAT-16).

use crate::layout::fragment::{Item, ShapeItem};
use crate::layout::traverse::paint_items;

use super::{fonts, package, package_bytes};

const NS: &str = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape" xmlns:wpg="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup""#;
pub(crate) const PT: i64 = 12700;

fn xfrm(x: i64, y: i64, w: i64, h: i64, extra: &str) -> String {
    format!(r#"<a:xfrm{extra}><a:off x="{}" y="{}"/><a:ext cx="{}" cy="{}"/></a:xfrm>"#, x * PT, y * PT, w * PT, h * PT)
}

pub(crate) fn sp(x: i64, y: i64, w: i64, h: i64, prst: &str, extra: &str, colour: &str) -> String {
    format!(
        r#"<wps:wsp><wps:cNvSpPr/><wps:spPr>{}<a:prstGeom prst="{prst}"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="{colour}"/></a:solidFill><a:ln w="12700"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln></wps:spPr><wps:bodyPr/></wps:wsp>"#,
        xfrm(x, y, w, h, extra)
    )
}

pub(crate) fn anchor(x: i64, y: i64, w: i64, h: i64, id: u32, behind: bool, z: u32, graphic: &str) -> String {
    let uri = if graphic.starts_with("<wpg:") {
        "http://schemas.microsoft.com/office/word/2010/wordprocessingGroup"
    } else {
        "http://schemas.microsoft.com/office/word/2010/wordprocessingShape"
    };
    format!(
        r#"<w:r><w:drawing><wp:anchor distT="0" distB="0" distL="0" distR="0" simplePos="0" relativeHeight="{z}" behindDoc="{}" locked="0" layoutInCell="1" allowOverlap="1"><wp:simplePos x="0" y="0"/><wp:positionH relativeFrom="page"><wp:posOffset>{}</wp:posOffset></wp:positionH><wp:positionV relativeFrom="page"><wp:posOffset>{}</wp:posOffset></wp:positionV><wp:extent cx="{}" cy="{}"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:wrapNone/><wp:docPr id="{id}" name="S{id}"/><a:graphic><a:graphicData uri="{uri}">{graphic}</a:graphicData></a:graphic></wp:anchor></w:drawing></w:r>"#,
        u8::from(behind),
        x * PT,
        y * PT,
        w * PT,
        h * PT
    )
}

fn doc(body: &str) -> Option<crate::docx::Document> {
    crate::docx::read(&bytes(body)?).ok()
}

/// A one-section letter-size document package with `body`.
pub(crate) fn bytes(body: &str) -> Option<Vec<u8>> {
    let ct = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#;
    let rels = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let document = format!(
        r#"<?xml version="1.0"?><w:document {NS}><w:body>{body}<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#
    );
    package_bytes(&[("[Content_Types].xml", ct), ("_rels/.rels", rels), ("word/document.xml", &document)])
}

fn shapes(items: &[Item]) -> Vec<&ShapeItem> {
    items.iter().filter_map(|i| if let Item::Shape(s) = i { Some(&**s) } else { None }).collect()
}

fn close(a: [f64; 4], b: [f64; 4]) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6)
}

#[test]
fn shapes_without_text_draw_at_their_place_with_their_rotation_and_flips() {
    let star = sp(0, 0, 90, 60, "star5", r#" rot="1800000" flipH="1""#, "FF0000");
    let body = format!("<w:p>{}</w:p>", anchor(100, 200, 90, 60, 1, false, 5, &star));
    let d = doc(&body).expect("the document reads");
    let snap = crate::layout::layout(&d, &fonts());
    let items = paint_items(&snap.pages()[0]);
    let s = shapes(&items);
    assert_eq!(s.len(), 1);
    assert!(close(s[0].rect, [100.0, 200.0, 190.0, 260.0]), "{:?}", s[0].rect);
    assert_eq!((s[0].rot, s[0].flip_h, s[0].flip_v), (30.0, true, false));
    assert!(!s[0].geom.paths.is_empty());
    assert!(matches!(s[0].fill, crate::drawing::paint::Paint::Solid(c) if c.to_rgb() == 0xFF0000));
    assert_eq!(s[0].line.width, 1.0);
}

#[test]
fn every_shape_of_a_nested_group_draws_at_its_mapped_place() {
    // A 200 x 100 pt group with a 100 x 100 child frame (x scale 2); inside, a group whose child frame scales y by 1.5.
    let inner = format!(
        r#"<wpg:grpSp><wpg:cNvPr id="9" name="g"/><wpg:cNvGrpSpPr/><wpg:grpSpPr><a:xfrm><a:off x="{}" y="0"/><a:ext cx="{}" cy="{}"/><a:chOff x="0" y="0"/><a:chExt cx="{}" cy="{}"/></a:xfrm></wpg:grpSpPr>{}</wpg:grpSp>"#,
        50 * PT,
        50 * PT,
        60 * PT,
        50 * PT,
        40 * PT,
        sp(0, 20, 10, 20, "ellipse", "", "00FF00")
    );
    let group = format!(
        r#"<wpg:wgp><wpg:cNvGrpSpPr/><wpg:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{}" cy="{}"/><a:chOff x="0" y="0"/><a:chExt cx="{}" cy="{}"/></a:xfrm></wpg:grpSpPr>{}{}</wpg:wgp>"#,
        200 * PT,
        100 * PT,
        100 * PT,
        100 * PT,
        sp(10, 10, 20, 30, "rect", "", "0000FF"),
        inner
    );
    let body = format!("<w:p>{}</w:p>", anchor(72, 100, 200, 100, 1, false, 5, &group));
    let d = doc(&body).expect("the document reads");
    let snap = crate::layout::layout(&d, &fonts());
    let items = paint_items(&snap.pages()[0]);
    let s = shapes(&items);
    assert_eq!(s.len(), 2, "both shapes of the group draw (FLOAT-15)");
    assert!(close(s[0].rect, [92.0, 110.0, 132.0, 140.0]), "{:?}", s[0].rect);
    assert_eq!(s[0].path, vec![0]);
    assert!(close(s[1].rect, [172.0, 130.0, 192.0, 160.0]), "{:?}", s[1].rect);
    assert_eq!(s[1].path, vec![1, 0]);
}

#[test]
fn behind_text_floats_draw_under_the_text_in_relative_height_order() {
    let a = anchor(72, 72, 100, 50, 1, true, 9, &sp(0, 0, 100, 50, "rect", "", "AA0000"));
    let b = anchor(72, 72, 100, 50, 2, true, 3, &sp(0, 0, 100, 50, "rect", "", "00AA00"));
    let c = anchor(72, 72, 100, 50, 3, false, 1, &sp(0, 0, 100, 50, "rect", "", "0000AA"));
    let body = format!(r#"<w:p>{a}{b}{c}<w:r><w:t>Text over the shapes</w:t></w:r></w:p>"#);
    let d = doc(&body).expect("the document reads");
    let snap = crate::layout::layout(&d, &fonts());
    let items = paint_items(&snap.pages()[0]);
    let order: Vec<String> = items
        .iter()
        .filter_map(|i| match i {
            Item::Shape(s) => match s.fill {
                crate::drawing::paint::Paint::Solid(c) => Some(format!("{:06X}", c.to_rgb())),
                _ => None,
            },
            Item::Glyphs(g) if !g.glyphs.is_empty() => Some("text".into()),
            _ => None,
        })
        .fold(Vec::new(), |mut v: Vec<String>, k| {
            if v.last() != Some(&k) {
                v.push(k);
            }
            v
        });
    // FLOAT-16: the behind floats lowest relativeHeight first, then the text, then the float in front.
    assert_eq!(order, ["00AA00", "AA0000", "text", "0000AA"]);
}

#[test]
fn a_hidden_shape_draws_nothing() {
    let star = sp(0, 0, 90, 60, "star5", "", "FF0000");
    let body = format!("<w:p>{}</w:p>", anchor(100, 200, 90, 60, 1, false, 5, &star).replace(r#"name="S1"/>"#, r#"name="S1" hidden="1"/>"#));
    let d = doc(&body).expect("the document reads");
    let snap = crate::layout::layout(&d, &fonts());
    assert!(shapes(&paint_items(&snap.pages()[0])).is_empty());
}

fn box_wsp(w: i64, h: i64, extra_xfrm: &str, body: &str, text: &str) -> String {
    format!(
        r#"<wps:wsp><wps:cNvSpPr txBox="1"/><wps:spPr>{}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val="FFFFFF"/></a:solidFill><a:ln w="12700"><a:solidFill><a:srgbClr val="000000"/></a:solidFill></a:ln></wps:spPr><wps:txbx><w:txbxContent><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:txbxContent></wps:txbx>{body}</wps:wsp>"#,
        xfrm(0, 0, w, h, extra_xfrm)
    )
}

fn settings_doc(body: &str, compat: u32) -> Option<crate::docx::Document> {
    let ct = r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/></Types>"#;
    let rels = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#;
    let drels = r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/settings" Target="settings.xml"/></Relationships>"#;
    let settings = format!(
        r#"<?xml version="1.0"?><w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="{compat}"/></w:compat></w:settings>"#
    );
    let document = format!(
        r#"<?xml version="1.0"?><w:document {NS}><w:body>{body}<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#
    );
    package(&[
        ("[Content_Types].xml", ct),
        ("_rels/.rels", rels),
        ("word/_rels/document.xml.rels", drels),
        ("word/settings.xml", &settings),
        ("word/document.xml", &document),
    ])
}

fn float_rect(snap: &crate::layout::Snapshot) -> [f64; 4] {
    fn find(children: &[crate::layout::fragment::Child]) -> Option<[f64; 4]> {
        children.iter().find_map(|c| match c {
            crate::layout::fragment::Child::Box(b) => match b.kind {
                crate::layout::fragment::BoxKind::Float { rect, .. } => Some(rect),
                _ => find(&b.children),
            },
            _ => None,
        })
    }
    snap.pages()[0]
        .children
        .iter()
        .find_map(|c| match c {
            crate::layout::fragment::PageChild::Body(b) => find(&b.children),
            _ => None,
        })
        .expect("a float")
}

#[test]
fn a_shape_turned_a_quarter_is_placed_as_its_turned_box_except_in_compat_12() {
    // FLOAT-17: 100 x 200 pt at (72, 560) turned 90°: placed at 22..222 x 610..710 (its centre stays).
    let shape = box_wsp(100, 200, r#" rot="5400000""#, "<wps:bodyPr/>", "Turned");
    let body = format!("<w:p>{}</w:p>", anchor(72, 560, 100, 200, 1, false, 1, &shape));
    let d = settings_doc(&body, 15).expect("the document reads");
    let snap = crate::layout::layout(&d, &fonts());
    assert!(close(float_rect(&snap), [22.0, 610.0, 222.0, 710.0]), "{:?}", float_rect(&snap));
    let items = paint_items(&snap.pages()[0]);
    let s = shapes(&items);
    assert!(close(s[0].rect, [72.0, 560.0, 172.0, 760.0]), "drawn at its own extent: {:?}", s[0].rect);
    assert_eq!(s[0].rot, 90.0);
    assert!(items.iter().any(|i| matches!(i, Item::Turned(t) if (t.deg - 90.0).abs() < 1e-9)), "its text is drawn turned");
    let d = settings_doc(&body, 12).expect("the document reads");
    assert!(close(float_rect(&crate::layout::layout(&d, &fonts())), [72.0, 560.0, 172.0, 760.0]));
}

#[test]
fn an_autofit_box_grows_from_its_stored_size_and_a_box_that_does_not_wrap_widens() {
    // FLOAT-10: stored 20 pt high; the frame (insets 3.6 + 3.6, pen 0.5 + 0.5) is 12.8 of it, so the height is 7.2 +
    // the content.
    let auto = box_wsp(200, 20, "", "<wps:bodyPr><a:spAutoFit/></wps:bodyPr>", "One line");
    let body = format!("<w:p>{}</w:p>", anchor(72, 100, 200, 20, 1, false, 1, &auto));
    let d = settings_doc(&body, 15).expect("the document reads");
    let r = float_rect(&crate::layout::layout(&d, &fonts()));
    let h = r[3] - r[1];
    assert!(h > 20.0 && h < 40.0, "grows to its one line: {h}");
    // FLOAT-18: a box 30 pt wide with a long line that doesn't wrap widens to it.
    let wide = box_wsp(30, 40, "", r#"<wps:bodyPr wrap="none"/>"#, "A line far wider than thirty points");
    let body = format!("<w:p>{}</w:p>", anchor(72, 100, 30, 40, 1, false, 1, &wide));
    let d = settings_doc(&body, 15).expect("the document reads");
    let r = float_rect(&crate::layout::layout(&d, &fonts()));
    assert!(r[2] - r[0] > 100.0, "{r:?}");
}

#[test]
fn vertical_text_is_turned_into_its_frame() {
    // FLOAT-19: vert270 turns the text 270°.
    let v = box_wsp(90, 72, "", r#"<wps:bodyPr vert="vert270"/>"#, "Upward");
    let body = format!("<w:p>{}</w:p>", anchor(72, 100, 90, 72, 1, false, 1, &v));
    let d = settings_doc(&body, 15).expect("the document reads");
    let items = paint_items(&crate::layout::layout(&d, &fonts()).pages()[0]);
    assert!(items.iter().any(|i| matches!(i, Item::Turned(t) if (t.deg - 270.0).abs() < 1e-9)));
}

#[test]
fn effects_resolve_from_the_effect_list_and_the_style() {
    // DRAW-17: a star with its own glow, soft edge and shadow; a rect taking the theme's third effect (Word's default
    // theme: an outer shadow) through its style reference.
    let effects = r#"<a:effectLst><a:outerShdw blurRad="50800" dist="38100" dir="2700000"><a:srgbClr val="000000"><a:alpha val="40000"/></a:srgbClr></a:outerShdw><a:glow rad="101600"><a:srgbClr val="FFC000"/></a:glow><a:softEdge rad="63500"/></a:effectLst>"#;
    let star = sp(0, 0, 90, 60, "star5", "", "FF0000").replace("</a:ln></wps:spPr>", &format!("</a:ln>{effects}</wps:spPr>"));
    let styled = r#"<wps:wsp><wps:cNvSpPr/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1143000" cy="762000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></wps:spPr><wps:style><a:lnRef idx="1"><a:schemeClr val="accent1"/></a:lnRef><a:fillRef idx="1"><a:schemeClr val="accent1"/></a:fillRef><a:effectRef idx="3"><a:schemeClr val="accent1"/></a:effectRef><a:fontRef idx="minor"><a:schemeClr val="lt1"/></a:fontRef></wps:style><wps:bodyPr/></wps:wsp>"#;
    let body = format!("<w:p>{}{}</w:p>", anchor(100, 200, 90, 60, 1, false, 1, &star), anchor(300, 200, 90, 60, 2, false, 2, styled));
    let d = doc(&body).expect("the document reads");
    let snap = crate::layout::layout(&d, &fonts());
    let items = paint_items(&snap.pages()[0]);
    let s = shapes(&items);
    let e = &s[0].effects;
    let shadow = e.outer.as_ref().expect("a shadow");
    assert_eq!((shadow.blur, shadow.dist, shadow.dir), (4.0, 3.0, 45.0));
    assert!((shadow.colour.a as f64 - 102.0).abs() <= 1.0, "40% alpha: {:?}", shadow.colour);
    assert_eq!(e.glow.map(|g| g.0), Some(8.0));
    assert_eq!(e.soft_edge, Some(5.0));
    let theme = s[1].effects.outer.as_ref().expect("the theme's third effect: a shadow");
    assert_eq!((theme.blur, theme.dist), (4.5, 1.5));
}
