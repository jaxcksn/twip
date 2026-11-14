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
    let ns = r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape""#;
    let body = format!("<w:p>{}</w:p>", anchor(100, 100, w, h, 1, false, 1, graphic));
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
            br#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId5" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="media/image1.png"/></Relationships>"#,
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
    let d = crate::docx::read(&package(graphic, size, image)).expect("the document reads");
    let snap = crate::layout::layout(&d, &fonts());
    let mut g = Graphics::default();
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
fn a_picture_fill_without_its_image_draws_a_light_frame() {
    let fill = format!(r#"<a:blipFill><a:blip r:embed="rId9"/>{STRETCH}</a:blipFill>{NO_LINE}"#);
    let s = draw(&wsp("rect", &fill));
    let edge = s.column(100.0, 120.0, 140.0);
    assert!(edge.iter().any(|p| (180..230).contains(&p[0]) && p[0] == p[1]), "a grey frame");
    assert!(paper(s.at(150.0, 130.0)), "nothing inside");
}
