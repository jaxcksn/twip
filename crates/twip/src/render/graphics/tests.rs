//! Shapes drawn on the CPU from DrawingML: each fill, line and effect leaves its mark where the shape's paint puts it.
//! One shape, 100 x 60 pt, at (100, 100) on the page; the window drawn is the page from (60, 60), 180 x 140 pt at 2
//! px per pt.

use std::io::Write as _;

use twip_graphics::cpu::{Format, Target};
use twip_graphics::kurbo::Affine;
use twip_graphics::paint::Color;

use super::Graphics;
use crate::layout::tests::fonts;
use crate::layout::tests::shapes::{PT, anchor};
use crate::render::Renderer;

const K: f64 = 2.0;
const ORIGIN: (f64, f64) = (60.0, 60.0);
const SIZE: (usize, usize) = (360, 280);
const RED: &str = r#"<a:solidFill><a:srgbClr val="FF0000"/></a:solidFill>"#;
const NO_LINE: &str = "<a:ln><a:noFill/></a:ln>";

/// The drawn window's pixels (premultiplied RGBA over white, so straight).
struct Shot(Vec<u8>);

impl Shot {
    /// The pixel at page point `(x, y)`, in pt.
    fn at(&self, x: f64, y: f64) -> [u8; 4] {
        let (px, py) = (((x - ORIGIN.0) * K) as usize, ((y - ORIGIN.1) * K) as usize);
        let i = (py * SIZE.0 + px) * 4;
        [self.0[i], self.0[i + 1], self.0[i + 2], self.0[i + 3]]
    }

    /// The pixels along the row at page `y` from `x0` to `x1` pt, one per device pixel.
    fn row(&self, y: f64, x0: f64, x1: f64) -> Vec<[u8; 4]> {
        (0..((x1 - x0) * K) as usize).map(|i| self.at(x0 + i as f64 / K, y)).collect()
    }

    /// The pixels down the column at page `x` from `y0` to `y1` pt, one per device pixel.
    fn column(&self, x: f64, y0: f64, y1: f64) -> Vec<[u8; 4]> {
        (0..((y1 - y0) * K) as usize).map(|i| self.at(x, y0 + i as f64 / K)).collect()
    }
}

fn paper(p: [u8; 4]) -> bool {
    p[..3].iter().all(|&v| v >= 250)
}

fn ink(p: [u8; 4]) -> bool {
    p[..3].iter().all(|&v| v <= 40)
}

fn red(p: [u8; 4]) -> bool {
    p[0] >= 240 && p[1] <= 15 && p[2] <= 15
}

/// A `wps:wsp` of preset `prst`, 100 x 60 pt (`w` x `h` when given), with `sp_pr` after its geometry.
fn wsp(prst: &str, sp_pr: &str) -> String {
    sized(prst, 100, 60, "", sp_pr)
}

fn sized(prst: &str, w: i64, h: i64, xfrm: &str, sp_pr: &str) -> String {
    format!(
        r#"<wps:wsp><wps:cNvSpPr/><wps:spPr><a:xfrm{xfrm}><a:off x="0" y="0"/><a:ext cx="{}" cy="{}"/></a:xfrm><a:prstGeom prst="{prst}"><a:avLst/></a:prstGeom>{sp_pr}</wps:spPr><wps:bodyPr/></wps:wsp>"#,
        w * PT,
        h * PT
    )
}

/// The document of `graphic` anchored at (100, 100) on the page, `w` x `h` pt, with `image` as `media/image1.png`
/// (relationship `rId5`).
fn package(graphic: &str, (w, h): (i64, i64), image: Option<&[u8]>) -> Vec<u8> {
    package_body(&format!("<w:p>{}</w:p>", anchor(100, 100, w, h, 1, false, 1, graphic)), image)
}

/// The document whose body is `body`, with `image` as `media/image1.png` (relationship `rId5`).
fn package_body(body: &str, image: Option<&[u8]>) -> Vec<u8> {
    let ns = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape""#;
    let document = format!(
        r#"<?xml version="1.0"?><w:document {ns}><w:body>{body}<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="720" w:footer="720" w:gutter="0"/></w:sectPr></w:body></w:document>"#
    );
    let parts: [(&str, &[u8]); 5] = [
        (
            "[Content_Types].xml",
            br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#,
        ),
        (
            "_rels/.rels",
            br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
        ),
        (
            "word/_rels/document.xml.rels",
            br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/><Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="file:///missing.png" TargetMode="External"/></Relationships>"#,
        ),
        ("word/document.xml", document.as_bytes()),
        ("word/media/image1.png", image.unwrap_or_default()),
    ];
    let mut z = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, data) in parts.iter().filter(|(n, _)| image.is_some() || !n.ends_with(".png")) {
        z.start_file(*name, zip::write::SimpleFileOptions::default()).expect("a part");
        z.write_all(data).expect("its bytes");
    }
    z.finish().expect("the package").into_inner()
}

/// `graphic`, `w` x `h` pt at (100, 100), drawn on the CPU.
fn draw_sized(graphic: &str, size: (i64, i64), image: Option<&[u8]>) -> Shot {
    draw_in(&mut Graphics::default(), graphic, size, image)
}

/// [`draw_sized`] with the graphics `g`, as a session draws a second document with the first one's.
fn draw_in(g: &mut Graphics, graphic: &str, size: (i64, i64), image: Option<&[u8]>) -> Shot {
    let d = crate::docx::read(&package(graphic, size, image)).expect("the document reads");
    let snap = crate::layout::layout(&d, &fonts());
    let picture = g.page_picture(&Renderer::default(), &snap.pages()[0], &d.images);
    let mut px = vec![0u8; SIZE.0 * SIZE.1 * 4];
    let mut target = Target::new(&mut px, SIZE.0 as u32, SIZE.1 as u32, SIZE.0 * 4, Format::Rgba8).expect("a target");
    g.render_picture(&mut target, &picture, Affine::scale(K) * Affine::translate((-ORIGIN.0, -ORIGIN.1)), Color::WHITE);
    Shot(px)
}

fn draw(graphic: &str) -> Shot {
    draw_sized(graphic, (100, 60), None)
}

/// A shape filled red, without a line, with `effects` (an `a:effectLst`'s children).
fn with_effects(effects: &str) -> Shot {
    draw(&wsp("rect", &format!("{RED}{NO_LINE}<a:effectLst>{effects}</a:effectLst>")))
}

#[test]
fn a_plain_shape_fills_its_box_and_nothing_else() {
    let s = with_effects("");
    assert!(red(s.at(101.0, 101.0)) && red(s.at(199.0, 159.0)), "{:?}", s.at(101.0, 101.0));
    assert!(paper(s.at(98.0, 130.0)) && paper(s.at(203.0, 163.0)) && paper(s.at(150.0, 162.0)));
}

#[test]
fn an_outer_shadow_falls_along_its_direction() {
    // 8 pt down and right, opaque: past the shape's bottom right corner, the shadow; above it, paper.
    let hard = with_effects(r#"<a:outerShdw dist="101600" dir="2700000" algn="tl" rotWithShape="0"><a:srgbClr val="000000"/></a:outerShdw>"#);
    assert!(ink(hard.at(203.0, 163.0)), "{:?}", hard.at(203.0, 163.0));
    assert!(paper(hard.at(203.0, 103.0)) && paper(hard.at(103.0, 163.0)));
    assert!(red(hard.at(150.0, 130.0)), "the shape stays over its shadow");
    // At 40% and blurred: grey, and fading at its edge.
    let soft = with_effects(
        r#"<a:outerShdw blurRad="101600" dist="101600" dir="2700000"><a:srgbClr val="000000"><a:alpha val="40000"/></a:srgbClr></a:outerShdw>"#,
    );
    let grey = soft.at(203.0, 140.0)[0];
    assert!((120..230).contains(&grey), "{grey}");
    assert!(soft.at(208.0, 140.0)[0] > grey, "the blur fades outward");
    // Scaled to half about the centre, turned with the shape: it no longer reaches past the corner.
    let scaled = with_effects(
        r#"<a:outerShdw dist="101600" dir="2700000" sx="50000" sy="50000" kx="600000" algn="ctr"><a:srgbClr val="000000"/></a:outerShdw>"#,
    );
    assert!(paper(scaled.at(203.0, 163.0)), "{:?}", scaled.at(203.0, 163.0));
}

#[test]
fn a_glow_rings_the_shape_in_its_colour() {
    let s = with_effects(r#"<a:glow rad="127000"><a:srgbClr val="00B050"/></a:glow>"#);
    let [r, g, b, _] = s.at(97.0, 130.0);
    assert!(g > r.saturating_add(30) && g > b.saturating_add(30), "green outside: {:?}", (r, g, b));
    assert!(paper(s.at(80.0, 130.0)), "it fades within its radius");
    assert!(red(s.at(150.0, 130.0)));
}

#[test]
fn a_soft_edge_fades_the_shape_toward_its_outline() {
    let s = with_effects(r#"<a:softEdge rad="127000"/>"#);
    assert!(red(s.at(150.0, 130.0)), "{:?}", s.at(150.0, 130.0));
    let (edge, inside) = (s.at(100.5, 130.0)[1], s.at(104.0, 130.0)[1]);
    assert!(edge > 200, "the outline fades to the paper: {edge}");
    assert!(inside < edge && inside > 15, "partway in, partway faded: {inside}");
    // Its shadow is faded with it.
    let shadowed = with_effects(r#"<a:outerShdw dist="101600" dir="2700000"><a:srgbClr val="000000"/></a:outerShdw><a:softEdge rad="127000"/>"#);
    let hard = with_effects(r#"<a:outerShdw dist="101600" dir="2700000"><a:srgbClr val="000000"/></a:outerShdw>"#);
    assert!(shadowed.at(203.0, 140.0)[0] > hard.at(203.0, 140.0)[0], "a softer shadow");
}

#[test]
fn an_inner_shadow_darkens_one_inside_edge() {
    let s = with_effects(r#"<a:innerShdw dist="76200" dir="2700000"><a:srgbClr val="000000"/></a:innerShdw>"#);
    let (top_left, bottom_right) = (s.at(102.0, 102.0), s.at(198.0, 158.0));
    assert!(ink(top_left) != ink(bottom_right), "one corner shaded: {top_left:?} {bottom_right:?}");
    assert!(red(s.at(150.0, 130.0)), "the middle is untouched");
    assert!(paper(s.at(98.0, 98.0)) && paper(s.at(202.0, 162.0)), "nothing outside");
}

#[test]
fn a_blur_softens_the_whole_shape() {
    let s = with_effects(r#"<a:blur rad="127000"/>"#);
    let edge = s.at(100.0, 130.0);
    assert!(!red(edge) && !paper(edge), "{edge:?}");
    assert!(red(s.at(150.0, 130.0)));
}

#[test]
fn a_pattern_fill_repeats_its_tile_in_both_colours() {
    let s = draw(&wsp(
        "rect",
        &format!(
            r#"<a:pattFill prst="horz"><a:fgClr><a:srgbClr val="000000"/></a:fgClr><a:bgClr><a:srgbClr val="FFFFFF"/></a:bgClr></a:pattFill>{NO_LINE}"#
        ),
    ));
    let col = s.column(150.0, 110.0, 150.0);
    assert!(col.iter().any(|p| ink(*p)) && col.iter().any(|p| paper(*p)), "stripes");
    assert!(paper(s.at(150.0, 165.0)), "only inside");
    // The cube's faces are shaded: its pattern's black and white are drawn darker and lighter there.
    let cube = draw(&wsp(
        "cube",
        &format!(
            r#"<a:pattFill prst="pct50"><a:fgClr><a:srgbClr val="000000"/></a:fgClr><a:bgClr><a:srgbClr val="FFFFFF"/></a:bgClr></a:pattFill>{NO_LINE}"#
        ),
    ));
    assert_ne!(cube.0, s.0);
}

fn gradient(prst: &str, kind: &str, extra: &str) -> Shot {
    draw(&wsp(
        prst,
        &format!(
            r#"<a:gradFill{extra}><a:gsLst><a:gs pos="0"><a:srgbClr val="FF0000"/></a:gs><a:gs pos="100000"><a:srgbClr val="0000FF"/></a:gs></a:gsLst>{kind}</a:gradFill>{NO_LINE}"#
        ),
    ))
}

#[test]
fn a_linear_gradient_runs_from_its_first_stop_to_its_last() {
    let s = gradient("rect", r#"<a:lin ang="0" scaled="0"/>"#, "");
    let (left, right) = (s.at(101.0, 130.0), s.at(199.0, 130.0));
    assert!(left[0] > 200 && left[2] < 60 && right[2] > 200 && right[0] < 60, "{left:?} {right:?}");
    // Scaled along the box's diagonal, with three stops: still red to blue.
    let three = draw(&wsp(
        "rect",
        &format!(
            r#"<a:gradFill><a:gsLst><a:gs pos="0"><a:srgbClr val="FF0000"/></a:gs><a:gs pos="50000"><a:srgbClr val="00FF00"/></a:gs><a:gs pos="100000"><a:srgbClr val="0000FF"/></a:gs></a:gsLst><a:lin ang="2700000" scaled="1"/></a:gradFill>{NO_LINE}"#
        ),
    ));
    assert!(three.at(150.0, 130.0)[1] > 150, "green in the middle: {:?}", three.at(150.0, 130.0));
    // A tile rect half the box's width: the ramp repeats, or mirrors when flipped.
    let tiled = gradient("rect", r#"<a:lin ang="0" scaled="0"/><a:tileRect r="50000"/>"#, "");
    let mirrored = gradient("rect", r#"<a:lin ang="0" scaled="0"/><a:tileRect r="50000"/>"#, r#" flip="x""#);
    assert!(tiled.at(199.0, 130.0)[2] > 200, "{:?}", tiled.at(199.0, 130.0));
    assert!(mirrored.at(199.0, 130.0)[0] > 200, "{:?}", mirrored.at(199.0, 130.0));
    // Turned with the shape off: the shape turns, its ramp stays across the page.
    let turned = draw(&sized(
        "rect",
        100,
        60,
        r#" rot="10800000""#,
        &format!(
            r#"<a:gradFill rotWithShape="0"><a:gsLst><a:gs pos="0"><a:srgbClr val="FF0000"/></a:gs><a:gs pos="100000"><a:srgbClr val="0000FF"/></a:gs></a:gsLst><a:lin ang="0" scaled="0"/></a:gradFill>{NO_LINE}"#
        ),
    ));
    assert!(turned.at(101.0, 130.0)[0] > 200, "{:?}", turned.at(101.0, 130.0));
}

#[test]
fn path_gradients_spread_from_their_focus() {
    let focus = r#"<a:fillToRect l="50000" t="50000" r="50000" b="50000"/>"#;
    for (prst, path) in [("rect", "circle"), ("rect", "rect"), ("rect", "shape"), ("ellipse", "shape")] {
        let s = gradient(prst, &format!(r#"<a:path path="{path}">{focus}</a:path>"#), "");
        let (centre, out) = (s.at(150.0, 130.0), s.at(150.0, 101.5));
        assert!(centre[0] > 200 && centre[2] < 60, "{prst} {path}: red at the focus, {centre:?}");
        assert!(out[2] > centre[2] + 60, "{prst} {path}: bluer toward the edge, {out:?}");
    }
}

/// A rectangle without fill, outlined by `ln` (an `a:ln`).
fn outlined(ln: &str) -> Shot {
    draw(&wsp("rect", &format!("<a:noFill/>{ln}")))
}

#[test]
fn dashed_lines_leave_gaps_along_the_outline() {
    let black = r#"<a:solidFill><a:srgbClr val="000000"/></a:solidFill>"#;
    for (dash, extra) in [
        (r#"<a:prstDash val="dash"/>"#, r#" cap="flat""#),
        (r#"<a:prstDash val="sysDot"/>"#, r#" cap="rnd""#),
        (r#"<a:prstDash val="lgDashDotDot"/>"#, r#" cap="sq""#),
        (r#"<a:custDash><a:ds d="300000" sp="200000"/></a:custDash>"#, ""),
    ] {
        for join in ["<a:round/>", "<a:bevel/>", r#"<a:miter lim="800000"/>"#] {
            let s = outlined(&format!(r#"<a:ln w="25400"{extra}>{black}{dash}{join}</a:ln>"#));
            let along = s.row(100.0, 110.0, 190.0);
            assert!(along.iter().any(|p| ink(*p)) && along.iter().any(|p| paper(*p)), "{dash} {join}: dashes and gaps");
        }
    }
    let solid = outlined(&format!(r#"<a:ln w="25400">{black}</a:ln>"#));
    assert!(solid.row(100.0, 110.0, 190.0).iter().all(|p| ink(*p)), "a solid line has none");
}

#[test]
fn compound_lines_draw_their_bands_with_gaps_between() {
    let black = r#"<a:solidFill><a:srgbClr val="000000"/></a:solidFill>"#;
    // 12 pt wide, across the top edge (y 94 to 106).
    let bands = |cmpd: &str, algn: &str| {
        let s = outlined(&format!(r#"<a:ln w="152400" cmpd="{cmpd}"{algn}>{black}</a:ln>"#));
        s.column(150.0, 92.0, 114.0).iter().map(|p| ink(*p)).collect::<Vec<bool>>()
    };
    let runs = |v: &[bool]| v.windows(2).filter(|w| w[0] && !w[1]).count() + usize::from(v.last() == Some(&true));
    assert_eq!(runs(&bands("dbl", "")), 2);
    assert_eq!(runs(&bands("thickThin", "")), 2);
    assert_eq!(runs(&bands("thinThick", "")), 2);
    assert_eq!(runs(&bands("tri", "")), 3);
    // Inset: the whole line inside the box, nothing above its top edge.
    let inset = bands("dbl", r#" algn="in""#);
    assert!(!inset[..10].iter().any(|i| *i), "{inset:?}");
    assert_eq!(runs(&inset), 2);
    // A gradient line: its paint over the bands' coverage, red at the left and blue at the right.
    let s = outlined(
        r#"<a:ln w="50800"><a:gradFill><a:gsLst><a:gs pos="0"><a:srgbClr val="FF0000"/></a:gs><a:gs pos="100000"><a:srgbClr val="0000FF"/></a:gs></a:gsLst><a:lin ang="0" scaled="0"/></a:gradFill></a:ln>"#,
    );
    let (left, right) = (s.at(100.0, 130.0), s.at(200.0, 130.0));
    assert!(left[0] > 150 && left[2] < 100 && right[2] > 150 && right[0] < 100, "{left:?} {right:?}");
    assert!(paper(s.at(150.0, 130.0)), "no fill");
}

/// A horizontal line from (100, 100) to (200, 100), 2 pt wide, with `ends` in its `a:ln`, and `cmpd`.
fn line(ends: &str, cmpd: &str) -> Shot {
    let ln = format!(r#"<a:ln w="25400"{cmpd}><a:solidFill><a:srgbClr val="000000"/></a:solidFill>{ends}</a:ln>"#);
    draw_sized(&sized("line", 100, 0, "", &ln), (100, 0), None)
}

#[test]
fn arrowheads_widen_an_open_line_at_its_ends() {
    // The plain shaft: 2 pt, so 4 pt off the line is paper at both ends.
    let plain = line("", "");
    assert!(ink(plain.at(150.0, 100.0)) || ink(plain.at(150.0, 100.5)), "the shaft: {:?}", plain.at(150.0, 100.0));
    for (head, tail) in [("triangle", "oval"), ("stealth", "arrow"), ("diamond", "triangle")] {
        let s = line(&format!(r#"<a:headEnd type="{head}" w="lg" len="lg"/><a:tailEnd type="{tail}" w="lg" len="lg"/>"#), "");
        assert_ne!(s.0, plain.0, "{head} {tail}");
        let near_head = s.column(103.0, 94.0, 106.0).iter().filter(|p| ink(**p)).count();
        let shaft = s.column(150.0, 94.0, 106.0).iter().filter(|p| ink(**p)).count();
        assert!(near_head > shaft, "{head}: wider than the shaft at the head ({near_head} against {shaft})");
    }
    // A double line has its bands on an open path too.
    let double = line("", r#" cmpd="dbl""#);
    assert_ne!(double.0, plain.0);
}

/// An 8 x 8 PNG: its left half red, its right half blue; the top row's left half white.
fn picture() -> Vec<u8> {
    let mut px = Vec::new();
    for y in 0..8 {
        for x in 0..8 {
            px.extend(match (x < 4, y == 0) {
                (true, true) => [255, 255, 255, 255],
                (true, false) => [255, 0, 0, 255],
                (false, _) => [0, 0, 255, 255],
            });
        }
    }
    crate::render::svg::png(8, 8, &px)
}

/// A rectangle filled with the picture, `blip` holding the `a:blip`'s children and `mode` its stretch or tile.
fn blip(adjust: &str, mode: &str, src: &str) -> Shot {
    let fill = format!(r#"<a:blipFill><a:blip r:embed="rId5">{adjust}</a:blip>{src}{mode}</a:blipFill>{NO_LINE}"#);
    draw_sized(&wsp("rect", &fill), (100, 60), Some(&picture()))
}

const STRETCH: &str = "<a:stretch><a:fillRect/></a:stretch>";

#[test]
fn a_picture_fill_stretches_its_image_over_the_box() {
    let s = blip("", STRETCH, "");
    assert!(red(s.at(120.0, 140.0)), "{:?}", s.at(120.0, 140.0));
    assert!(s.at(180.0, 140.0)[2] > 240 && s.at(180.0, 140.0)[0] < 20, "{:?}", s.at(180.0, 140.0));
    // Cropped to its right half: blue across the box.
    let cropped = blip("", STRETCH, r#"<a:srcRect l="50000"/>"#);
    assert!(cropped.at(120.0, 140.0)[2] > 240, "{:?}", cropped.at(120.0, 140.0));
}

#[test]
fn picture_adjustments_recolour_the_image() {
    let at = |s: &Shot| s.at(120.0, 140.0);
    let grey = at(&blip("<a:grayscl/>", STRETCH, ""));
    assert!(grey[0] == grey[1] && grey[1] == grey[2], "{grey:?}");
    let two = at(&blip(r#"<a:biLevel thresh="50000"/>"#, STRETCH, ""));
    assert!(two[..3].iter().all(|&v| v == two[0]) && (two[0] == 0 || two[0] == 255), "{two:?}");
    let duo = at(&blip(r#"<a:duotone><a:srgbClr val="000000"/><a:srgbClr val="00FF00"/></a:duotone>"#, STRETCH, ""));
    assert!(duo[0] < 10 && duo[2] < 10 && duo[1] > 20, "{duo:?}");
    let bright = at(&blip(r#"<a:lum bright="40000" contrast="-20000"/>"#, STRETCH, ""));
    assert!(bright[1] > 60, "{bright:?}");
    let contrast = at(&blip(r#"<a:lum contrast="40000"/>"#, STRETCH, ""));
    assert!(red(contrast), "{contrast:?}");
    let faded = at(&blip(r#"<a:alphaModFix amt="50000"/>"#, STRETCH, ""));
    assert!(faded[0] > 240 && (100..160).contains(&faded[1]), "red at half over white: {faded:?}");
}

#[test]
fn a_vector_picture_fill_takes_its_adjustments() {
    let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"><rect width="8" height="8" fill="#FF0000"/></svg>"##;
    let shot = |adjust: &str, mode: &str| {
        let fill = format!(r#"<a:blipFill><a:blip r:embed="rId5">{adjust}</a:blip>{mode}</a:blipFill>{NO_LINE}"#);
        draw_sized(&wsp("rect", &fill), (100, 60), Some(svg)).at(120.0, 140.0)
    };
    for mode in [STRETCH, r#"<a:tile tx="0" ty="0" sx="100000" sy="100000" flip="none" algn="tl"/>"#] {
        assert!(red(shot("", mode)), "{mode}: {:?}", shot("", mode));
        let grey = shot("<a:grayscl/>", mode);
        assert!(grey[0] == grey[1] && grey[1] == grey[2] && grey[0] < 200, "{mode}: {grey:?}");
        let faded = shot(r#"<a:alphaModFix amt="50000"/>"#, mode);
        assert!(faded[0] > 240 && (100..160).contains(&faded[1]), "{mode}: red at half over white: {faded:?}");
    }
}

#[test]
fn a_tiled_picture_repeats_at_its_scale_from_its_alignment() {
    // Each tile is 8 px at 75%, doubled: 12 pt, so a row across 30 pt crosses red and blue.
    for algn in ["tl", "t", "tr", "l", "ctr", "r", "bl", "b", "br"] {
        let s = blip("", &format!(r#"<a:tile tx="0" ty="0" sx="200000" sy="200000" flip="none" algn="{algn}"/>"#), "");
        let row = s.row(130.0, 110.0, 140.0);
        assert!(row.iter().any(|p| red(*p)) && row.iter().any(|p| p[2] > 240 && p[0] < 20), "{algn}");
    }
    // Recoloured, a tiled picture is drawn in a layer that carries the filter.
    let grey = blip("<a:grayscl/>", r#"<a:tile sx="50000" sy="50000"/>"#, "");
    assert!(grey.row(130.0, 110.0, 140.0).iter().all(|p| p[0] == p[1] && p[1] == p[2]));
}

#[test]
fn a_tiled_metafile_repeats_as_vectors_or_as_its_pixels() {
    use crate::render::metafile::tests::Emf;
    let null_pen = 0x8000_0008u32.cast_signed();
    // Red on its left half, blue on its right: 72 pt square.
    let emf = Emf::default().r(37, &[null_pen]).r(39, &[1, 0, 0xFF, 0]).r(37, &[1]).r(43, &[0, 0, 50, 100]);
    let emf = emf.r(39, &[2, 0, 0xFF_0000, 0]).r(37, &[2]).r(43, &[50, 0, 100, 100]).bytes();
    let tiled = |sx: &str| {
        let fill = format!(r#"<a:blipFill><a:blip r:embed="rId5"/><a:tile sx="{sx}" sy="{sx}" flip="none" algn="tl"/></a:blipFill>{NO_LINE}"#);
        draw_sized(&sized("rect", 170, 130, "", &fill), (170, 130), Some(&emf))
    };
    // A tenth: tiles 7.2 pt across, each placed. Three hundredths: 2.16 pt, too many to place, repeated as pixels.
    for sx in ["10000", "3000"] {
        let row = tiled(sx).row(130.0, 101.0, 140.0);
        let (reds, blues) = (row.iter().any(|p| p[0] > 200 && p[2] < 80), row.iter().any(|p| p[2] > 200 && p[0] < 80));
        assert!(reds && blues, "{sx}: red and blue across");
    }
    let s = tiled("10000");
    // Tile 0's left quarter and tile 1's: the same red.
    assert!(red(s.at(101.8, 105.0)) && red(s.at(109.0, 105.0)) && s.at(105.4, 105.0)[2] > 200, "{:?}", s.at(105.4, 105.0));
}

#[test]
fn a_picture_fill_without_its_image_draws_a_light_frame() {
    let fill = format!(r#"<a:blipFill><a:blip r:embed="rId9"/>{STRETCH}</a:blipFill>{NO_LINE}"#);
    let s = draw(&wsp("rect", &fill));
    let edge = s.column(100.0, 120.0, 140.0);
    assert!(edge.iter().any(|p| (180..230).contains(&p[0]) && p[0] == p[1]), "a grey frame");
    assert!(paper(s.at(150.0, 130.0)), "nothing inside");
}

#[test]
fn a_new_document_lets_the_last_ones_images_go() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/FX-OBJ.docx");
    let doc = crate::docx::read(&std::fs::read(path).expect("fixture")).expect("reads");
    let mut g = super::Graphics::default();
    let key = crate::layout::fragment::ImageKey { index: Some(0), generation: 0 };
    let id = g.image(key, &doc.images).expect("an image");
    g.clear_images();
    assert!(g.res.images.info(id).is_none(), "the registry holds the old document's bytes no longer");
}

#[test]
fn a_linked_picture_draws_its_placeholder_and_its_embedded_copy_when_it_has_one() {
    let draw_blip =
        |blip: &str| draw_sized(&wsp("rect", &format!(r#"<a:blipFill>{blip}{STRETCH}</a:blipFill>{NO_LINE}"#)), (100, 60), Some(&picture()));
    // Only a link: a light frame and nothing inside, though the package holds an image.
    let linked = draw_blip(r#"<a:blip r:link="rId9"/>"#);
    assert!(paper(linked.at(120.0, 140.0)) && paper(linked.at(180.0, 140.0)), "no picture without its embedded copy");
    assert!(linked.column(100.0, 120.0, 140.0).iter().any(|p| (180..230).contains(&p[0]) && p[0] == p[1]), "a grey frame");
    // A link and its embedded copy: the copy.
    let both = draw_blip(r#"<a:blip r:link="rId9" r:embed="rId5"/>"#);
    assert!(red(both.at(120.0, 140.0)) && both.at(180.0, 140.0)[2] > 240, "the embedded picture");
}

#[test]
fn a_changed_colour_becomes_transparent_within_one_level() {
    let change = |from: &str, to: &str| {
        blip(&format!(r#"<a:clrChange><a:clrFrom><a:srgbClr val="{from}"/></a:clrFrom><a:clrTo>{to}</a:clrTo></a:clrChange>"#), STRETCH, "")
    };
    let clear = r#"<a:srgbClr val="FF0000"><a:alpha val="0"/></a:srgbClr>"#;
    let s = change("FF0000", clear);
    assert!(paper(s.at(120.0, 140.0)), "red goes: {:?}", s.at(120.0, 140.0));
    assert!(s.at(180.0, 140.0)[2] > 240 && s.at(180.0, 140.0)[0] < 20, "blue stays");
    let near = change("FE0101", clear);
    assert!(paper(near.at(120.0, 140.0)), "a level off in a channel is the colour: {:?}", near.at(120.0, 140.0));
    let far = change("FD0000", clear);
    assert!(red(far.at(120.0, 140.0)), "two levels off is not: {:?}", far.at(120.0, 140.0));
    let green = change("FF0000", r#"<a:srgbClr val="00FF00"/>"#);
    assert!(green.at(120.0, 140.0)[1] > 240 && green.at(120.0, 140.0)[0] < 20, "drawn as the new colour: {:?}", green.at(120.0, 140.0));
}

#[test]
fn a_picture_that_does_not_decode_keeps_its_colours() {
    // A PNG whose header claims 10000 x 10000 pixels over 8 x 8 of data: it doesn't decode, and costs nothing.
    let mut png = picture();
    png[16..24].copy_from_slice(&[0, 0, 0x27, 0x10, 0, 0, 0x27, 0x10]);
    let crc = {
        let mut c = 0xFFFF_FFFFu32;
        for b in &png[12..29] {
            c ^= u32::from(*b);
            for _ in 0..8 {
                c = if c & 1 == 1 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 };
            }
        }
        !c
    };
    png[29..33].copy_from_slice(&crc.to_be_bytes());
    let images = vec![png];
    let mut g = Graphics::default();
    let key = crate::layout::fragment::ImageKey { index: Some(0), generation: 0 };
    let plain = g.image(key, &images).expect("registered by its header");
    let from = crate::drawing::colour::Rgba::rgb(0xFF0000);
    assert_eq!(g.recoloured(key, from, from, true, &images), Some(plain));
    assert_eq!(g.recoloured_bytes, 0);
}

#[test]
fn a_reflection_is_a_faded_copy_below_the_shape() {
    let r = |attrs: &str| with_effects(&format!(r#"<a:reflection {attrs}/>"#));
    let s = r(r#"stA="60000" endA="0" endPos="100000" dir="5400000" sy="-100000" algn="b" rotWithShape="0""#);
    assert!(red(s.at(150.0, 130.0)), "the shape stays");
    let near = s.at(150.0, 161.0);
    let far = s.at(150.0, 195.0);
    assert!(near[0] > 240 && (60..160).contains(&near[1]), "red at 60% under the shape: {near:?}");
    assert!(far[1] > near[1], "fading with distance: {near:?} {far:?}");
    assert!(paper(s.at(80.0, 170.0)), "nothing beside it");
    // A gap of 10 pt, then the copy.
    let apart = r(r#"stA="100000" endA="100000" dist="127000" dir="5400000" sy="-100000" algn="b" rotWithShape="0""#);
    assert!(paper(apart.at(150.0, 165.0)) && red(apart.at(150.0, 175.0)), "{:?} {:?}", apart.at(150.0, 165.0), apart.at(150.0, 175.0));
    // The fade runs the way fadeDir says: along x it leaves the left of a copy to its right opaque, the rest clear.
    let side = r(r#"stA="100000" endA="0" endPos="50000" dir="0" fadeDir="0" sx="-100000" algn="r" rotWithShape="0""#);
    assert!(side.at(205.0, 130.0)[1] < 40 && paper(side.at(255.0, 130.0)), "{:?} {:?}", side.at(205.0, 130.0), side.at(255.0, 130.0));
    assert!(paper(with_effects("").at(150.0, 170.0)), "no reflection without the effect");
}

#[test]
fn hostile_reflections_draw_something_bounded() {
    for attrs in [
        r#"sx="0" sy="0""#,
        r#"sy="-100000000000" dist="9000000000000" dir="5400000""#,
        r#"blurRad="9000000000000" stA="100000" endA="100000""#,
        r#"stPos="90000" endPos="10000" stA="100000" endA="0" fadeDir="123456789""#,
        r#"kx="5399999" ky="-5399999" sy="-100000""#,
        r#"stA="-5" endA="999999999" stPos="-1" endPos="99999999999""#,
    ] {
        let s = with_effects(&format!("<a:reflection {attrs}/>"));
        assert!(red(s.at(150.0, 130.0)), "the shape still draws with {attrs}");
    }
}

#[test]
fn an_absurd_blur_radius_draws_as_the_clamped_one() {
    // Radii in the billions of points: drawn as the largest radius Twip draws (its bound is asserted where it resolves),
    // whatever the effect.
    let max = 200 * PT;
    let plain = with_effects("");
    for (absurd, clamped) in [
        (
            r#"<a:outerShdw blurRad="9000000000000" dist="9000000000000" dir="2700000"><a:srgbClr val="000000"/></a:outerShdw>"#.to_string(),
            format!(r#"<a:outerShdw blurRad="{max}" dist="{max}" dir="2700000"><a:srgbClr val="000000"/></a:outerShdw>"#),
        ),
        (
            r#"<a:innerShdw blurRad="9000000000000" dist="1270000"><a:srgbClr val="000000"/></a:innerShdw>"#.to_string(),
            format!(r#"<a:innerShdw blurRad="{max}" dist="1270000"><a:srgbClr val="000000"/></a:innerShdw>"#),
        ),
        (
            r#"<a:glow rad="9000000000000"><a:srgbClr val="00FF00"/></a:glow>"#.to_string(),
            format!(r#"<a:glow rad="{max}"><a:srgbClr val="00FF00"/></a:glow>"#),
        ),
        (r#"<a:softEdge rad="9000000000000"/>"#.to_string(), format!(r#"<a:softEdge rad="{max}"/>"#)),
        (r#"<a:blur rad="9000000000000"/>"#.to_string(), format!(r#"<a:blur rad="{max}"/>"#)),
    ] {
        let (s, max_drawn) = (with_effects(&absurd), with_effects(&clamped));
        assert_ne!(max_drawn.0, plain.0, "the clamped effect shows: {clamped}");
        assert_eq!(s.0, max_drawn.0, "{absurd}");
    }
}

#[test]
fn a_reflection_fades_along_a_flipped_or_turned_shape() {
    let r = |xfrm: &str, attrs: &str| {
        let fill = format!("{RED}{NO_LINE}<a:effectLst><a:reflection {attrs}/></a:effectLst>");
        draw(&sized("rect", 100, 60, xfrm, &fill))
    };
    // Flipped about its x axis the shape's copy is above it, and, turning with it, so is its fade: strongest at the
    // shape's edge, clear far from it.
    let up = r(r#" flipV="1""#, r#"stA="100000" endA="0" dir="5400000" sy="-100000" algn="b" rotWithShape="1""#);
    assert!(up.at(150.0, 99.0)[1] < 60 && up.at(150.0, 70.0)[1] > up.at(150.0, 99.0)[1], "{:?} {:?}", up.at(150.0, 99.0), up.at(150.0, 70.0));
    // Not turning with it, the same reflection lies below and fades downward.
    let down = r(r#" flipV="1""#, r#"stA="100000" endA="0" dir="5400000" sy="-100000" algn="b" rotWithShape="0""#);
    assert!(down.at(150.0, 161.0)[1] < 60 && down.at(150.0, 190.0)[1] > down.at(150.0, 161.0)[1], "{:?}", down.at(150.0, 161.0));
}

#[test]
fn an_inline_pictures_reflection_stops_at_its_paragraph() {
    let at = |after: &str| {
        let d = crate::docx::read(&inline_package(after, "")).expect("reads");
        let snap = crate::layout::layout(&d, &fonts());
        let mut g = Graphics::default();
        let picture = g.page_picture(&Renderer::default(), &snap.pages()[0], &d.images);
        let mut px = vec![0u8; SIZE.0 * SIZE.1 * 4];
        let mut target = Target::new(&mut px, SIZE.0 as u32, SIZE.1 as u32, SIZE.0 * 4, Format::Rgba8).expect("a target");
        g.render_picture(&mut target, &picture, Affine::scale(K) * Affine::translate((-ORIGIN.0, -ORIGIN.1)), Color::WHITE);
        Shot(px)
    };
    // The picture is 100 x 60 pt at the page's margin (72, 72); its reflection falls 60 pt below, faded.
    let tight = at("0");
    let roomy = at("400");
    let below = |s: &Shot, y: f64| s.at(100.0, y);
    assert!(!paper(below(&roomy, 145.0)), "20 pt of spacing after holds the reflection: {:?}", below(&roomy, 145.0));
    assert!(paper(below(&roomy, 160.0)), "and no more: {:?}", below(&roomy, 160.0));
    assert!(paper(below(&tight, 140.0)), "no spacing, no reflection past the line: {:?}", below(&tight, 140.0));
}

/// A document of one paragraph holding an inline 100 x 60 pt picture with a reflection, `after` twips of spacing after.
fn inline_package(after: &str, tail: &str) -> Vec<u8> {
    let graphic =
        format!(r#"<w:p><w:pPr><w:spacing w:before="0" w:after="{after}" w:line="240" w:lineRule="auto"/></w:pPr>{}{tail}</w:p>"#, INLINE_REFLECTED);
    package_body(&graphic, Some(&picture()))
}

const CLEAR_RED: &str = r#"<a:blipFill><a:blip r:embed="rId5"><a:clrChange><a:clrFrom><a:srgbClr val="FF0000"/></a:clrFrom><a:clrTo><a:srgbClr val="FF0000"><a:alpha val="0"/></a:srgbClr></a:clrTo></a:clrChange></a:blip><a:stretch><a:fillRect/></a:stretch></a:blipFill><a:ln><a:noFill/></a:ln>"#;

#[test]
fn a_new_document_does_not_draw_the_last_ones_changed_pictures() {
    let rect = wsp("rect", CLEAR_RED);
    let green = crate::render::svg::png(8, 8, &[0, 255, 0, 255].repeat(64));
    let mut g = Graphics::default();
    let first = draw_in(&mut g, &rect, (100, 60), Some(&picture()));
    assert!(paper(first.at(120.0, 140.0)), "the first document's red goes");
    g.clear_images();
    // The same picture index, colour and change: a different image.
    let second = draw_in(&mut g, &rect, (100, 60), Some(&green));
    assert!(second.at(120.0, 140.0)[1] > 240 && second.at(120.0, 140.0)[0] < 20, "its own pixels: {:?}", second.at(120.0, 140.0));
}

#[test]
fn changed_pictures_in_use_stay_over_the_cap_and_unused_ones_go() {
    let rect = wsp("rect", CLEAR_RED);
    let mut g = Graphics::default();
    let first = draw_in(&mut g, &rect, (100, 60), Some(&picture()));
    assert_eq!((g.recolour_decodes, g.recoloured.len()), (1, 1));
    let id = g.recoloured.values().next().and_then(|r| r.id).expect("decoded");
    // Over the cap (0), frame after frame of the same pages: nothing is dropped, nothing is decoded again.
    for _ in 0..3 {
        g.trim_recoloured(0);
        let again = draw_in(&mut g, &rect, (100, 60), Some(&picture()));
        assert_eq!(first.0, again.0);
    }
    assert_eq!((g.recolour_decodes, g.recoloured.len()), (1, 1), "kept while drawn");
    // Two frames that don't draw it: gone, bytes and all, and made again when it is drawn.
    g.trim_recoloured(0);
    g.trim_recoloured(0);
    assert!(g.recoloured.is_empty() && g.recoloured_bytes == 0 && g.res.images.info(id).is_none());
    let back = draw_in(&mut g, &rect, (100, 60), Some(&picture()));
    assert_eq!((first.0, g.recolour_decodes), (back.0, 2));
}

#[test]
fn an_inline_reflection_runs_to_the_bottom_of_a_longer_paragraph() {
    let d = crate::docx::read(&inline_package("0", r#"<w:r><w:br/></w:r><w:r><w:t>second line</w:t></w:r>"#)).expect("reads");
    let snap = crate::layout::layout(&d, &fonts());
    let mut g = Graphics::default();
    let picture = g.page_picture(&Renderer::default(), &snap.pages()[0], &d.images);
    let mut px = vec![0u8; SIZE.0 * SIZE.1 * 4];
    let mut target = Target::new(&mut px, SIZE.0 as u32, SIZE.1 as u32, SIZE.0 * 4, Format::Rgba8).expect("a target");
    g.render_picture(&mut target, &picture, Affine::scale(K) * Affine::translate((-ORIGIN.0, -ORIGIN.1)), Color::WHITE);
    let s = Shot(px);
    assert!(!paper(s.at(165.0, 134.0)), "the picture's line ends, its paragraph goes on: {:?}", s.at(165.0, 134.0));
}

#[test]
fn a_shapes_reflection_holds_its_text() {
    let shape = |text: &str| {
        format!(
            r#"<wps:wsp><wps:cNvSpPr txBox="0"/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{}" cy="{}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom>{RED}{NO_LINE}<a:effectLst><a:reflection stA="100000" endA="100000" dir="5400000" sy="-100000" algn="b" rotWithShape="0"/></a:effectLst></wps:spPr><wps:txbx><w:txbxContent><w:p><w:r><w:rPr><w:color w:val="FFFFFF"/><w:sz w:val="80"/></w:rPr><w:t>{text}</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp>"#,
            100 * PT,
            60 * PT
        )
    };
    let below = |s: &Shot| (0..36).flat_map(|i| s.row(162.0 + f64::from(i), 100.0, 200.0)).filter(|p| !red(*p) && !paper(*p)).count();
    let plain = draw(&shape(""));
    let texted = draw(&shape("MMMM"));
    assert_eq!(below(&plain), 0, "a bare shape reflects red only");
    assert!(below(&texted) > 50, "the white text is in the reflection: {}", below(&texted));
}

/// A floated group of two 100 x 60 pt shapes side by side: the first with a reflection (and `text` when it has any), the
/// second with `text` in it when the first has none.
fn reflected_group(text_in_reflected: bool) -> String {
    let leaf = |x: i64, reflected: bool, text: &str| {
        let effects = if reflected {
            r#"<a:effectLst><a:reflection stA="100000" endA="100000" dir="5400000" sy="-100000" algn="b" rotWithShape="0"/></a:effectLst>"#
        } else {
            ""
        };
        let txbx = if text.is_empty() {
            String::new()
        } else {
            format!(
                r#"<wps:txbx><w:txbxContent><w:p><w:r><w:rPr><w:color w:val="FFFFFF"/><w:sz w:val="80"/></w:rPr><w:t>{text}</w:t></w:r></w:p></w:txbxContent></wps:txbx>"#
            )
        };
        format!(
            r#"<wps:wsp><wps:cNvSpPr txBox="0"/><wps:spPr><a:xfrm><a:off x="{}" y="0"/><a:ext cx="{}" cy="{}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom>{RED}{NO_LINE}{effects}</wps:spPr>{txbx}<wps:bodyPr/></wps:wsp>"#,
            x * PT,
            100 * PT,
            60 * PT
        )
    };
    let (first, second) = if text_in_reflected { ("MMMM", "") } else { ("", "MMMM") };
    format!(
        r#"<wpg:wgp xmlns:wpg="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup"><wpg:cNvGrpSpPr/><wpg:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{w}" cy="{h}"/><a:chOff x="0" y="0"/><a:chExt cx="{w}" cy="{h}"/></a:xfrm></wpg:grpSpPr>{}{}</wpg:wgp>"#,
        leaf(0, true, first),
        leaf(100, false, second),
        w = 200 * PT,
        h = 60 * PT
    )
}

/// The reflected shapes of a page: whether each holds text in its reflection.
fn reflections_with_text(doc: &crate::docx::Document) -> Vec<bool> {
    let snap = crate::layout::layout(doc, &fonts());
    crate::layout::traverse::paint_items(&snap.pages()[0])
        .into_iter()
        .filter_map(|i| match i {
            crate::layout::fragment::Item::Shape(s) if s.effects.reflection.is_some() => Some(s.text.is_some()),
            _ => None,
        })
        .collect()
}

#[test]
fn a_leafs_reflection_holds_its_own_text_and_not_its_siblings() {
    let doc = |text_in_reflected| {
        let body = format!("<w:p>{}</w:p>", anchor(100, 100, 200, 60, 1, false, 1, &reflected_group(text_in_reflected)));
        crate::docx::read(&package_body(&body, None)).expect("reads")
    };
    assert_eq!(reflections_with_text(&doc(true)), [true], "the reflected leaf's text");
    assert_eq!(reflections_with_text(&doc(false)), [false], "the text belongs to the sibling");
}

const INLINE_REFLECTED: &str = r#"<w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="1270000" cy="762000"/><wp:docPr id="1" name="P"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:nvPicPr><pic:cNvPr id="1" name="p.png"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rId5"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1270000" cy="762000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:effectLst><a:reflection stA="100000" endA="100000" dir="5400000" sy="-100000" algn="b" rotWithShape="0"/></a:effectLst></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#;

#[test]
fn an_inline_reflection_stops_at_the_part_of_a_split_paragraph_on_its_page() {
    // 40 lines, the picture's line, then 40 more: the paragraph splits, and the picture is on the first page, near its end.
    let br = "<w:r><w:br/></w:r>".repeat(40);
    let body = format!(r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="auto"/></w:pPr>{br}{INLINE_REFLECTED}{br}</w:p>"#);
    let d = crate::docx::read(&package_body(&body, Some(&picture()))).expect("reads");
    let snap = crate::layout::layout(&d, &fonts());
    assert!(snap.pages().len() >= 2, "the paragraph splits");
    let clips: Vec<f64> = crate::layout::traverse::paint_items(&snap.pages()[0])
        .into_iter()
        .filter_map(|i| if let crate::layout::fragment::Item::Shape(s) = i { s.clip_bottom } else { None })
        .collect();
    assert_eq!(clips.len(), 1);
    // The text area ends at 720 pt; the part's last line ends within a line of it (a 12 pt line is under 14 pt).
    assert!(clips[0] <= 720.5 && 720.0 - clips[0] < 14.0, "clipped at the page's text area, not the paragraph's last line: {}", clips[0]);
}

#[test]
fn a_reflected_picture_in_an_inline_group_stops_at_its_paragraph_too() {
    let pic = INLINE_REFLECTED
        .split_once(r#"<pic:pic "#)
        .and_then(|(_, rest)| rest.split_once("</pic:pic>"))
        .map(|(inner, _)| format!("<pic:pic {inner}</pic:pic>"))
        .expect("the picture");
    let group = format!(
        r#"<w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="1270000" cy="762000"/><wp:docPr id="1" name="G"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup"><wpg:wgp xmlns:wpg="http://schemas.microsoft.com/office/word/2010/wordprocessingGroup"><wpg:cNvGrpSpPr/><wpg:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1270000" cy="762000"/><a:chOff x="0" y="0"/><a:chExt cx="1270000" cy="762000"/></a:xfrm></wpg:grpSpPr>{pic}</wpg:wgp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>"#
    );
    let body = format!(r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0" w:line="240" w:lineRule="auto"/></w:pPr>{group}</w:p>"#);
    let d = crate::docx::read(&package_body(&body, Some(&picture()))).expect("reads");
    let snap = crate::layout::layout(&d, &fonts());
    let shapes: Vec<_> = crate::layout::traverse::paint_items(&snap.pages()[0])
        .into_iter()
        .filter_map(|i| if let crate::layout::fragment::Item::Shape(s) = i { Some(s) } else { None })
        .filter(|s| s.picture && s.effects.reflection.is_some())
        .collect();
    assert_eq!(shapes.len(), 1, "the group's picture is a reflected leaf");
    assert!(shapes[0].clip_bottom.is_some(), "its reflection is cut at its paragraph");
}

#[test]
fn an_inline_text_boxs_reflection_holds_its_text() {
    let run = |text: &str| {
        format!(
            r#"<w:p><w:pPr><w:spacing w:before="0" w:after="2000"/></w:pPr><w:r><w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{w}" cy="{h}"/><wp:docPr id="1" name="T"/><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/word/2010/wordprocessingShape"><wps:wsp><wps:cNvSpPr txBox="0"/><wps:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="{w}" cy="{h}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom>{RED}{NO_LINE}<a:effectLst><a:reflection stA="100000" endA="100000" dir="5400000" sy="-100000" algn="b" rotWithShape="0"/></a:effectLst></wps:spPr><wps:txbx><w:txbxContent><w:p><w:r><w:rPr><w:color w:val="FFFFFF"/><w:sz w:val="80"/></w:rPr><w:t>{text}</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"#,
            w = 100 * PT,
            h = 60 * PT
        )
    };
    let shot = |text: &str| {
        let d = crate::docx::read(&package_body(&run(text), None)).expect("reads");
        let snap = crate::layout::layout(&d, &fonts());
        let mut g = Graphics::default();
        let picture = g.page_picture(&Renderer::default(), &snap.pages()[0], &d.images);
        let mut px = vec![0u8; SIZE.0 * SIZE.1 * 4];
        let mut target = Target::new(&mut px, SIZE.0 as u32, SIZE.1 as u32, SIZE.0 * 4, Format::Rgba8).expect("a target");
        g.render_picture(&mut target, &picture, Affine::scale(K) * Affine::translate((-ORIGIN.0, -ORIGIN.1)), Color::WHITE);
        Shot(px)
    };
    let below = |s: &Shot| (0..30).flat_map(|i| s.row(134.0 + f64::from(i), 74.0, 170.0)).filter(|p| !red(*p) && !paper(*p)).count();
    assert_eq!(below(&shot("")), 0, "a bare box reflects red only");
    assert!(below(&shot("MMMM")) > 50, "the text is in the reflection: {}", below(&shot("MMMM")));
}

/// A VML text path in red, `w` x 60 pt at (100, 100) on the page: `textpath` holds the `v:textpath`'s attributes,
/// in JetBrains Mono.
fn text_path(w: i64, textpath: &str, shape: &str) -> Shot {
    let pict = format!(
        r##"<w:p><w:r><w:pict xmlns:v="urn:schemas-microsoft-com:vml" xmlns:o="urn:schemas-microsoft-com:office:office"><v:shapetype id="_x0000_t136" coordsize="21600,21600" o:spt="136" adj="10800" path="m@7,l@8,m@5,21600l@6,21600e"><v:path textpathok="t"/><v:textpath on="t" fitshape="t"/></v:shapetype><v:shape type="#_x0000_t136" style="position:absolute;margin-left:100pt;margin-top:100pt;width:{w}pt;height:60pt;mso-position-horizontal-relative:page;mso-position-vertical-relative:page{shape}" fillcolor="red" stroked="f"><v:textpath style="font-family:&quot;JetBrains Mono&quot;;font-size:1pt" {textpath}/></v:shape></w:pict></w:r></w:p>"##
    );
    let d = crate::docx::read(&package_body(&pict, None)).expect("the document reads");
    let snap = crate::layout::layout(&d, &crate::layout::tests::output::bundled_fonts());
    let mut g = Graphics::default();
    let picture = g.page_picture(&Renderer::default(), &snap.pages()[0], &d.images);
    let mut px = vec![0u8; SIZE.0 * SIZE.1 * 4];
    let mut target = Target::new(&mut px, SIZE.0 as u32, SIZE.1 as u32, SIZE.0 * 4, Format::Rgba8).expect("a target");
    g.render_picture(&mut target, &picture, Affine::scale(K) * Affine::translate((-ORIGIN.0, -ORIGIN.1)), Color::WHITE);
    Shot(px)
}

/// The page x of the first and last red pixels along the row at page `y`.
fn red_span(s: &Shot, y: f64) -> Option<(f64, f64)> {
    let row = s.row(y, ORIGIN.0, ORIGIN.0 + SIZE.0 as f64 / K);
    let x = |i: usize| ORIGIN.0 + i as f64 / K;
    Some((x(row.iter().position(|p| red(*p))?), x(row.iter().rposition(|p| red(*p))?)))
}

#[test]
fn a_text_path_stretches_its_glyphs_ink_to_its_box_less_the_insets() {
    // An `H`'s bar runs across its ink box at mid-height.
    let s = text_path(120, r#"string="H""#, "");
    let (x0, x1) = red_span(&s, 130.0).expect("the bar draws");
    assert!((x0 - 107.2).abs() <= 1.0 && (x1 - 212.8).abs() <= 1.0, "{x0} {x1}");
    // Its stems run from 3.6 pt below the box's top to 3.6 pt above its bottom.
    let stem = s.column(x0 + 1.0, 100.0, 160.0);
    let (top, bottom) = (stem.iter().position(|p| red(*p)).unwrap(), stem.iter().rposition(|p| red(*p)).unwrap());
    assert!((100.0 + top as f64 / K - 103.6).abs() <= 1.0 && (100.0 + bottom as f64 / K - 156.4).abs() <= 1.0, "{top} {bottom}");
    // Trimmed, the ink fills the whole box.
    let (x0, x1) = red_span(&text_path(120, r#"string="H" trim="t""#, ""), 130.0).expect("the bar draws");
    assert!((x0 - 100.0).abs() <= 1.0 && (x1 - 220.0).abs() <= 1.0, "{x0} {x1}");
}

#[test]
fn a_text_path_turns_with_its_shape_and_draws_nothing_unfitted_or_empty() {
    // A half turn puts the `L`'s foot at the top.
    let upright = text_path(120, r#"string="L" trim="t""#, "");
    let turned = text_path(120, r#"string="L" trim="t""#, ";rotation:180");
    let foot = |s: &Shot, y: f64| red_span(s, y).map(|(a, b)| b - a).unwrap_or(0.0);
    assert!(foot(&upright, 158.0) > 100.0 && foot(&upright, 102.0) < 40.0);
    assert!(foot(&turned, 102.0) > 100.0 && foot(&turned, 158.0) < 40.0);
    for blank in [r#"string="H" fitshape="f""#, r#"string="  ""#, ""] {
        let s = text_path(120, blank, "");
        assert!(red_span(&s, 130.0).is_none(), "{blank}");
        assert!(s.row(130.0, 100.0, 220.0).iter().all(|p| paper(*p)), "no box either: {blank}");
    }
    // The longest string read draws, squeezed into the box.
    let long = text_path(120, &format!(r#"string="{}""#, "W".repeat(5000)), "");
    assert!(long.row(130.0, 107.0, 213.0).iter().all(|p| !paper(*p)));
}

fn fixture(name: &str) -> crate::docx::Document {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/render").join(name);
    crate::docx::read(&std::fs::read(path).expect("fixture")).expect("reads")
}

#[test]
fn a_page_background_takes_its_theme_tint_or_shade_and_shows_only_when_word_shows_it() {
    // Word writes the theme colour it shows as `w:color` too.
    for (name, written) in [("page-colour.docx", 0xF4_B183u32), ("page-colour-shade.docx", 0x2F_5597)] {
        let d = fixture(name);
        let got = d.page_colour.expect("a page colour");
        assert!((0..3).all(|i| ((got >> (8 * i)) & 0xFF).abs_diff((written >> (8 * i)) & 0xFF) <= 1), "{name}: {got:06X}");
        assert_eq!(d.shown_background().0, Some(got));
    }
    let hidden = fixture("page-colour-no-shape.docx");
    assert!(hidden.page_colour.is_some() && hidden.shown_background() == (None, None), "no displayBackgroundShape");
    let mut r = Renderer::default();
    r.set_page_background(&hidden);
    assert!(r.page_colour.is_none() && r.page_fill.is_none());
    let gradient = fixture("page-gradient.docx");
    assert!(matches!(gradient.page_fill, Some(crate::drawing::paint::Fill::Gradient(_))));
    r.set_page_background(&gradient);
    assert!(matches!(r.page_fill, Some(crate::drawing::paint::Paint::Gradient { .. })));
    let picture = fixture("page-picture.docx");
    let Some(crate::drawing::paint::Fill::Blip(b)) = &picture.page_fill else { panic!("{:?}", picture.page_fill) };
    assert!(b.image.is_some_and(|i| i < picture.images.len()), "the tile's picture is read");
}

#[test]
fn a_picture_watermark_is_drawn_behind_the_body_text() {
    use crate::layout::fragment::Item;
    let d = fixture("watermark-picture.docx");
    let snap = crate::layout::layout(&d, &crate::layout::tests::output::bundled_fonts());
    for page in snap.pages() {
        let mut order = Vec::new();
        crate::layout::traverse::paint(page, &mut |p| match p.item {
            Item::Shape(s) if s.picture => order.push('p'),
            Item::Glyphs(_) => order.push('t'),
            _ => {}
        });
        assert!(order.contains(&'t'), "the page has body text: {order:?}");
        assert_eq!(order.first(), Some(&'p'), "{order:?}");
        assert_eq!(order.iter().filter(|c| **c == 'p').count(), 1);
    }
}

/// Spaced shaded rows and shaded paragraphs joined over their spacing meet at fractional pixels without a pale seam:
/// shading is drawn aliased.
#[test]
fn abutting_shading_leaves_no_seam() {
    let fill = [0x8E, 0xAA, 0xDB, 0xFF];
    let p = |shd: &str| {
        format!(
            r#"<w:p><w:pPr><w:shd w:val="clear" w:color="auto" w:fill="{shd}"/><w:spacing w:before="0" w:after="67" w:line="266" w:lineRule="exact"/></w:pPr><w:r><w:t>x</w:t></w:r></w:p>"#
        )
    };
    let tc = format!(r#"<w:tc><w:tcPr><w:tcW w:w="1500" w:type="dxa"/></w:tcPr>{}</w:tc>"#, p("FFFFFF"));
    let body = format!(
        r#"<w:tbl><w:tblPr><w:tblW w:w="3000" w:type="dxa"/><w:tblCellSpacing w:w="61" w:type="dxa"/><w:shd w:val="clear" w:color="auto" w:fill="8EAADB"/></w:tblPr><w:tblGrid><w:gridCol w:w="1500"/><w:gridCol w:w="1500"/></w:tblGrid>{}</w:tbl>{}"#,
        format!("<w:tr>{tc}{tc}</w:tr>").repeat(3),
        p("8EAADB").repeat(3)
    );
    let d = crate::docx::read(&package_body(&body, None)).expect("the document reads");
    let snap = crate::layout::layout(&d, &fonts());
    let page = &snap.pages()[0];
    let shaded: Vec<[f64; 4]> = crate::layout::traverse::paint_items(page)
        .into_iter()
        .filter_map(|i| match i {
            crate::layout::fragment::Item::Rect { rect, colour: 0x8EAADB, snap: true, .. } => Some(rect),
            _ => None,
        })
        .collect();
    // The table's gaps (left of 222 pt), then the paragraphs' lines.
    let (table, paras): (Vec<[f64; 4]>, Vec<[f64; 4]>) = shaded.iter().partition(|r| r[2] <= 222.0 + 1e-6);
    assert_eq!(paras.len(), 3, "{shaded:?}");
    assert!(!table.is_empty(), "the table's shading is laid out: {shaded:?}");
    let top = table.iter().map(|r| r[1]).fold(f64::MAX, f64::min);
    let bottom = table.iter().map(|r| r[3]).fold(f64::MIN, f64::max);
    let mut g = Graphics::default();
    let picture = g.page_picture(&Renderer::default(), page, &d.images);
    let mut px = vec![0u8; SIZE.0 * SIZE.1 * 4];
    let mut target = Target::new(&mut px, SIZE.0 as u32, SIZE.1 as u32, SIZE.0 * 4, Format::Rgba8).expect("a target");
    g.render_picture(&mut target, &picture, Affine::scale(K) * Affine::translate((-ORIGIN.0, -ORIGIN.1)), Color::WHITE);
    let shot = Shot(px);
    // Down the table's left gap, and the paragraphs clear of their text.
    for (x, top, bottom) in [(73.0, top, bottom), (200.0, paras[0][1], paras[2][3])] {
        let column = shot.column(x, top + 1.0, bottom - 1.0);
        assert!(!column.is_empty() && column.iter().all(|&c| c == fill), "{:?}", column.iter().filter(|&&c| c != fill).collect::<Vec<_>>());
    }
}

const SVG_RED: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><rect width="10" height="10" fill="#FF0000"/></svg>"##;

#[test]
fn a_stretched_svg_takes_its_pictures_opacity() {
    let at = |adjust: &str| {
        let fill = format!(r#"<a:blipFill><a:blip r:embed="rId5">{adjust}</a:blip>{STRETCH}</a:blipFill>{NO_LINE}"#);
        draw_sized(&wsp("rect", &fill), (100, 60), Some(SVG_RED.as_bytes())).at(150.0, 130.0)
    };
    assert!(red(at("")), "{:?}", at(""));
    let half = at(r#"<a:alphaModFix amt="50000"/>"#);
    assert!(half[0] > 240 && (100..160).contains(&half[1]), "red at half over white: {half:?}");
}

#[test]
fn a_color_change_without_alpha_keeps_each_pixels_alpha() {
    let half_red = crate::render::svg::png(8, 8, &[255, 0, 0, 128].repeat(64));
    let at = |use_a: &str| {
        let fill = format!(
            r#"<a:blipFill><a:blip r:embed="rId5"><a:clrChange{use_a}><a:clrFrom><a:srgbClr val="FF0000"/></a:clrFrom><a:clrTo><a:srgbClr val="0000FF"/></a:clrTo></a:clrChange></a:blip>{STRETCH}</a:blipFill>{NO_LINE}"#
        );
        draw_sized(&wsp("rect", &fill), (100, 60), Some(&half_red)).at(150.0, 130.0)
    };
    let kept = at("");
    assert!(kept[0] > 240 && (100..160).contains(&kept[1]), "alpha counts, a half-clear pixel isn't the opaque colour: {kept:?}");
    let blue = at(r#" useA="0""#);
    assert!(blue[2] > 240 && (100..160).contains(&blue[0]), "blue at the pixel's half alpha: {blue:?}");
}

#[test]
fn a_tiled_fill_with_no_width_or_mirrored_tiles_leaves_no_gaps() {
    use crate::render::metafile::tests::Emf;
    let null_pen = 0x8000_0008u32.cast_signed();
    let emf = Emf::default().r(37, &[null_pen]).r(39, &[1, 0, 0xFF, 0]).r(37, &[1]).r(43, &[0, 0, 50, 100]);
    let emf = emf.r(39, &[2, 0, 0xFF_0000, 0]).r(37, &[2]).r(43, &[50, 0, 100, 100]).bytes();
    // Tiles of 2.16 pt, each mirrored, from 1 pt in: the mosaic's columns meet.
    let fill =
        format!(r#"<a:blipFill><a:blip r:embed="rId5"/><a:tile tx="12700" ty="0" sx="3000" sy="3000" flip="x" algn="tl"/></a:blipFill>{NO_LINE}"#);
    let s = draw_sized(&sized("rect", 170, 130, "", &fill), (170, 130), Some(&emf));
    assert!(s.row(130.0, 110.0, 200.0).iter().all(|p| !paper(*p)), "no tile leaves a gap");
    // An area with no width on a tile edge, a trillion points tall, draws nothing and returns.
    let mut g = Graphics::default();
    let mut rec = twip_graphics::scene::Recorder::new();
    let key = crate::layout::fragment::ImageKey { index: Some(0), generation: 0 };
    let area = twip_graphics::kurbo::Rect::new(4.0, 0.0, 4.0, 1e12);
    g.vector_tiles(&mut rec, key, &[emf], (0.0, 0.0, 2.0, 2.0), [0.0; 4], crate::drawing::paint::TileFlip::None, area);
}

#[test]
fn a_namespaced_svg_is_a_vector_picture() {
    let svg = r##"<s:svg xmlns:s="http://www.w3.org/2000/svg" viewBox="0 0 10 10"><s:rect width="10" height="10" fill="#FF0000"/></s:svg>"##;
    assert_eq!(crate::docx::image::vector(svg.as_bytes()), Some(crate::docx::image::Vector::Svg));
    assert!(crate::docx::image::vector(b"<s:svgx/>").is_none() && crate::docx::image::vector(b"<svgs/>").is_none());
    let fill = format!(r#"<a:blipFill><a:blip r:embed="rId5"/>{STRETCH}</a:blipFill>{NO_LINE}"#);
    assert!(red(draw_sized(&wsp("rect", &fill), (100, 60), Some(svg.as_bytes())).at(150.0, 130.0)));
}

#[test]
fn a_page_thumbnail_fits_its_page_into_the_pixels() {
    let shape = sized("rect", 412, 592, "", r#"<a:solidFill><a:srgbClr val="000000"/></a:solidFill>"#);
    let d = crate::docx::read(&package(&shape, (412, 592), None)).expect("the document reads");
    let snap = crate::layout::layout(&d, &fonts());
    let mut g = Graphics::default();
    let grey = Color::rgba(128, 128, 128, 255);
    let mut px = vec![0u8; 100 * 100 * 4];
    // A letter page, 612 x 792 pt, is 77 x 100 px, 11 px in from each side.
    assert_eq!(g.page_thumbnail(&Renderer::default(), &snap.pages()[0], &d.images, &mut px, (100, 100), grey), Some((77, 100)));
    let at = |x: usize, y: usize| [px[(y * 100 + x) * 4], px[(y * 100 + x) * 4 + 1], px[(y * 100 + x) * 4 + 2]];
    assert_eq!(at(5, 50), [128, 128, 128], "around the page");
    assert_eq!(at(14, 5), [255, 255, 255], "the page's margin");
    assert_eq!(at(50, 50), [0, 0, 0], "its content");
    assert_eq!(g.page_thumbnail(&Renderer::default(), &snap.pages()[0], &d.images, &mut px, (101, 100), grey), None, "pixels of another size");
    let mut big = vec![0u8; 100 * 100 * 4 + 4];
    assert_eq!(g.page_thumbnail(&Renderer::default(), &snap.pages()[0], &d.images, &mut big, (100, 100), grey), None, "more pixels than the size");
    let mut empty = vec![0u8; 0x100];
    for size in [(0, 100), (100, 0), (0, 0)] {
        assert_eq!(g.page_thumbnail(&Renderer::default(), &snap.pages()[0], &d.images, &mut empty, size, grey), None, "an empty size");
    }
    assert_eq!(g.page_thumbnail(&Renderer::default(), &snap.pages()[0], &d.images, &mut [], (0, 0), grey), None, "no pixels");
}
