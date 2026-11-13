//! DrawingML shapes

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
    assert_eq!(s.len(), 2, "both shapes of the group draw");
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

#[test]
fn a_hidden_shape_wraps_nothing() {
    let star = sp(0, 0, 200, 100, "star5", "", "FF0000");
    let shown = anchor(72, 72, 200, 100, 1, false, 5, &star).replace("<wp:wrapNone/>", r#"<wp:wrapSquare wrapText="bothSides"/>"#);
    let hidden = shown.replace(r#"name="S1"/>"#, r#"name="S1" hidden="1"/>"#);
    let first_x = |a: &str| {
        let d = doc(&format!("<w:p>{a}<w:r><w:t>Words</w:t></w:r></w:p>")).expect("the document reads");
        super::page_chars(&d)[0][0].1
    };
    assert!(first_x(&shown) > 270.0, "a shown shape wraps the line");
    assert!((first_x(&hidden) - 72.0).abs() < 0.5, "a hidden one doesn't: {}", first_x(&hidden));
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
    let auto = box_wsp(200, 20, "", "<wps:bodyPr><a:spAutoFit/></wps:bodyPr>", "One line");
    let body = format!("<w:p>{}</w:p>", anchor(72, 100, 200, 20, 1, false, 1, &auto));
    let d = settings_doc(&body, 15).expect("the document reads");
    let r = float_rect(&crate::layout::layout(&d, &fonts()));
    let h = r[3] - r[1];
    assert!(h > 20.0 && h < 40.0, "grows to its one line: {h}");
    let wide = box_wsp(30, 40, "", r#"<wps:bodyPr wrap="none"/>"#, "A line far wider than thirty points");
    let body = format!("<w:p>{}</w:p>", anchor(72, 100, 30, 40, 1, false, 1, &wide));
    let d = settings_doc(&body, 15).expect("the document reads");
    let r = float_rect(&crate::layout::layout(&d, &fonts()));
    assert!(r[2] - r[0] > 100.0, "{r:?}");
}

#[test]
fn vertical_text_is_turned_into_its_frame() {
    let v = box_wsp(90, 72, "", r#"<wps:bodyPr vert="vert270"/>"#, "Upward");
    let body = format!("<w:p>{}</w:p>", anchor(72, 100, 90, 72, 1, false, 1, &v));
    let d = settings_doc(&body, 15).expect("the document reads");
    let items = paint_items(&crate::layout::layout(&d, &fonts()).pages()[0]);
    assert!(items.iter().any(|i| matches!(i, Item::Turned(t) if (t.deg - 270.0).abs() < 1e-9)));
}

/// A text box with no outline, its paragraphs in Arial 10 with single lines.
fn vertical_box(w: i64, h: i64, body: &str, paras: &[&str]) -> String {
    let ps: String = paras
        .iter()
        .map(|t| format!(r#"<w:p><w:pPr><w:spacing w:after="0" w:line="240" w:lineRule="auto"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:sz w:val="20"/></w:rPr><w:t>{t}</w:t></w:r></w:p>"#))
        .collect();
    format!(
        r#"<wps:wsp><wps:cNvSpPr txBox="1"/><wps:spPr>{}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></wps:spPr><wps:txbx><w:txbxContent>{ps}</w:txbxContent></wps:txbx>{body}</wps:wsp>"#,
        xfrm(0, 0, w, h, "")
    )
}

/// The first page's turned runs: text, origin and direction.
fn turned_runs(snap: &crate::layout::Snapshot) -> Vec<(String, [f64; 2], [f64; 2])> {
    let mut out = Vec::new();
    for it in paint_items(&snap.pages()[0]) {
        if let Item::Turned(t) = it {
            for (r, at, dir) in crate::layout::vertical::runs(&t) {
                out.push((r.chars.iter().map(|c| c.0).collect::<String>().trim().to_string(), at, dir));
            }
        }
    }
    out.retain(|r| !r.0.is_empty());
    out
}

#[test]
fn a_vert_text_box_lays_its_lines_out_like_a_vertical_cell_its_anchor_moving_them_across() {
    // Bottom-anchored: the block against the side the lines stack towards, the left one.
    let v = vertical_box(100, 200, r#"<wps:bodyPr vert="vert" lIns="0" tIns="0" rIns="0" bIns="0" anchor="b"/>"#, &["HHHH x", "second"]);
    let body = format!("<w:p>{}</w:p>", anchor(100, 100, 100, 200, 1, false, 1, &v));
    let d = settings_doc(&body, 15).expect("the document reads");
    let runs = turned_runs(&crate::layout::layout(&d, &fonts()));
    assert!(runs.len() == 2 && runs.iter().all(|r| r.2 == [0.0, 1.0]), "{runs:?}");
    // A vertical line's metrics: the last line its descent in from the left, the lines a vertical pitch apart.
    assert!((runs[1].1[0] - 102.26).abs() < 0.2 && (runs[0].1[0] - 114.26).abs() < 0.2, "{runs:?}");
    assert!(runs.iter().all(|r| (r.1[1] - 100.0).abs() < 0.1), "{runs:?}");
}

#[test]
fn a_word_art_vertical_text_box_stacks_its_lines_from_the_left() {
    let v = vertical_box(100, 200, r#"<wps:bodyPr vert="wordArtVert" lIns="0" tIns="0" rIns="0" bIns="0"/>"#, &["HHHH x", "second"]);
    let body = format!("<w:p>{}</w:p>", anchor(100, 100, 100, 200, 1, false, 1, &v));
    let d = settings_doc(&body, 15).expect("the document reads");
    let runs = turned_runs(&crate::layout::layout(&d, &fonts()));
    assert!(runs.len() == 2 && runs.iter().all(|r| r.2 == [0.0, 1.0]), "{runs:?}");
    assert!((runs[0].1[0] - 102.50).abs() < 0.25 && (runs[1].1[0] - 114.26).abs() < 0.2, "{runs:?}");
}

#[test]
fn a_vertical_text_box_that_fits_its_text_grows_across() {
    let v = vertical_box(20, 200, r#"<wps:bodyPr vert="vert" lIns="91440" tIns="45720" rIns="91440" bIns="45720"><a:spAutoFit/></wps:bodyPr>"#, &["one two", "three"]);
    let body = format!("<w:p>{}</w:p>", anchor(100, 100, 20, 200, 1, false, 1, &v));
    let d = settings_doc(&body, 15).expect("the document reads");
    let r = float_rect(&crate::layout::layout(&d, &fonts()));
    // Two vertical lines and the insets (no pen): 23.6 + 14.4.
    assert!((r[2] - r[0] - 38.0).abs() < 0.2 && (r[3] - r[1] - 200.0).abs() < 1e-6, "{r:?}");
}

#[test]
fn effects_resolve_from_the_effect_list_and_the_style() {
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

#[test]
fn a_relative_width_and_a_percentage_position_follow_the_page() {
    let shape = box_wsp(100, 50, "", "<wps:bodyPr/>", "Share");
    let pct = r#"<wp:positionH relativeFrom="margin"><mc:AlternateContent xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006"><mc:Choice Requires="wp14"><wp14:pctPosHOffset xmlns:wp14="http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing">25000</wp14:pctPosHOffset></mc:Choice><mc:Fallback><wp:posOffset>0</wp:posOffset></mc:Fallback></mc:AlternateContent></wp:positionH>"#;
    let rel = r#"<wp14:sizeRelH xmlns:wp14="http://schemas.microsoft.com/office/word/2010/wordprocessingDrawing" relativeFrom="page"><wp14:pctWidth>50000</wp14:pctWidth></wp14:sizeRelH></wp:anchor>"#;
    let a = anchor(72, 100, 100, 50, 1, false, 1, &shape);
    let a = a.replacen(&format!(r#"<wp:positionH relativeFrom="page"><wp:posOffset>{}</wp:posOffset></wp:positionH>"#, 72 * PT), pct, 1).replacen(
        "</wp:anchor>",
        rel,
        1,
    );
    let d = settings_doc(&format!("<w:p>{a}</w:p>"), 15).expect("the document reads");
    let r = float_rect(&crate::layout::layout(&d, &fonts()));
    assert!(close(r, [189.0, 100.0, 495.0, 150.0]), "{r:?}");
}

/// The float a VML rectangle with `style` and `extra` attributes reads as, in a document of `compat`.
fn vml_float(style: &str, extra: &str, wrap: &str, compat: u32) -> crate::docx::model::Float {
    let rect = format!(
        r#"<w:p><w:r><w:pict><v:rect xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office" xmlns:w10="urn:schemas-microsoft-com:office:word" style="position:absolute;margin-left:72pt;margin-top:72pt;width:90pt;height:60pt;{style}" {extra}>{wrap}</v:rect></w:pict></w:r></w:p>"#
    );
    let d = settings_doc(&rect, compat).expect("the document reads");
    let crate::docx::model::Block::Para(p) = &d.sections[0].blocks[0] else { panic!("a paragraph") };
    p.inlines.iter().find_map(|i| if let crate::docx::model::Inline::Float(f, _) = i { Some((**f).clone()) } else { None }).expect("the VML float")
}

#[test]
fn a_vml_floats_wrap_distances_round_to_twips_and_default_to_nine_points_beside() {
    let square = r#"<w10:wrap type="square"/>"#;
    assert_eq!(vml_float("", "", square, 15).dist, [0.0, 0.0, 9.0, 9.0]);
    let f = vml_float("mso-wrap-distance-top:0.03in;mso-wrap-distance-left:0pt;mso-wrap-distance-right:24px", "", square, 15);
    assert_eq!(f.dist, [2.15, 0.0, 0.0, 18.0]);
}

#[test]
fn a_vml_float_reads_its_overlap_and_cell_flags() {
    let f = vml_float("", r#"o:allowoverlap="f" o:allowincell="false""#, "", 14);
    assert!(!f.allow_overlap && !f.layout_in_cell);
    let f = vml_float("", r#"o:allowoverlap="t""#, "", 14);
    assert!(f.allow_overlap && f.layout_in_cell);
}

#[test]
fn a_compat_15_vml_float_is_behind_the_text_only_without_wrapping() {
    let behind = |wrap: &str, compat| vml_float("z-index:-3", "", wrap, compat).behind;
    let square = r#"<w10:wrap type="square"/>"#;
    assert!(!behind(square, 15) && behind("", 15) && behind(r#"<w10:wrap type="none"/>"#, 15));
    assert!(behind(square, 14));
}

#[test]
fn a_tight_vml_float_wraps_around_its_wrap_coordinates() {
    let coords = r#"wrapcoords="0 0 21600 0 10800 21600 0 0""#;
    let f = vml_float("", coords, r#"<w10:wrap type="tight"/>"#, 15);
    assert_eq!(f.poly.as_deref(), Some(&[(0.0, 0.0), (1.0, 0.0), (0.5, 1.0), (0.0, 0.0)][..]));
    assert!(vml_float("", coords, r#"<w10:wrap type="square"/>"#, 15).poly.is_none());
}

#[test]
fn a_linked_box_lays_its_part_of_the_story_out_in_its_own_text_rectangle() {
    // A hexagon heads the chain; the rectangle after it is as wide but its text rectangle is wider.
    let shape = |prst: &str, text: &str| {
        format!(
            r#"<wps:wsp><wps:cNvSpPr txBox="1"/><wps:spPr>{}<a:prstGeom prst="{prst}"><a:avLst/></a:prstGeom></wps:spPr>{text}<wps:bodyPr/></wps:wsp>"#,
            xfrm(0, 0, 150, 60, "")
        )
    };
    let paras: String = (1..=6).map(|k| format!("<w:p><w:r><w:t>Linked paragraph {k} with some words to fill the boxes in turn.</w:t></w:r></w:p>")).collect();
    let head = shape("hexagon", &format!(r#"<wps:txbx id="1"><w:txbxContent>{paras}</w:txbxContent></wps:txbx>"#));
    let next = shape("rect", r#"<wps:linkedTxbx id="1" seq="1"/>"#);
    let body = format!("<w:p>{}{}</w:p>", anchor(72, 100, 150, 60, 1, false, 1, &head), anchor(300, 100, 150, 200, 2, false, 2, &next));
    let d = settings_doc(&body, 15).expect("the document reads");
    let right = super::page_chars(&d)[0].iter().filter(|c| c.1 > 290.0).map(|c| c.1).fold(0.0, f64::max);
    assert!(right > 415.0, "the rectangle's lines run past the hexagon's text width: {right}");
}
