//! The EMF player: records drawn into a log and on the CPU, and hostile files.

use std::collections::HashMap;
use std::fmt::Write;
use std::time::{Duration, Instant};

use twip_graphics::kurbo::{self, Affine, BezPath, Shape};

use super::emf::{Header, play};
use crate::render::svg::{Brush, Sink, Text};

/// An EMF made record by record, on a 100 × 100 pixel device 25.4 mm across (a pixel is 0.72 pt) whose surface is
/// the frame: the picture is 72 pt square.
#[derive(Default)]
pub(crate) struct Emf {
    body: Vec<u8>,
    records: u32,
}

/// The stock `NULL_PEN`'s handle.
const NULL_PEN: i32 = 0x8000_0008u32.cast_signed();

pub(super) fn le(v: &[i32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

/// UTF-16LE.
fn wide(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

impl Emf {
    /// A record of `kind` holding `body`, padded to four bytes.
    pub(crate) fn rec(mut self, kind: u32, body: &[u8]) -> Emf {
        let mut b = body.to_vec();
        b.resize(b.len().next_multiple_of(4), 0);
        self.body.extend(kind.to_le_bytes());
        self.body.extend(((b.len() + 8) as u32).to_le_bytes());
        self.body.extend(b);
        self.records += 1;
        self
    }

    pub(crate) fn r(self, kind: u32, v: &[i32]) -> Emf {
        self.rec(kind, &le(v))
    }

    /// `EMR_CREATEBRUSHINDIRECT` of a solid `colour` at `ih`, selected.
    fn brush(self, ih: i32, colour: i32) -> Emf {
        self.r(39, &[ih, 0, colour, 0]).r(37, &[ih])
    }

    /// `EMR_EXTTEXTOUTW` of `s` at (`x`, `y`), each character `dx` apart.
    fn text(self, x: i32, y: i32, s: &str, dx: i32) -> Emf {
        let n = s.encode_utf16().count() as i32;
        let off_string = 76;
        let chars = wide(s);
        let off_dx = off_string + chars.len().next_multiple_of(4) as i32;
        let mut b = le(&[0, 0, 0, 0, 1, 0, 0, x, y, n, off_string, 0, 0, 0, 0, 0, off_dx]);
        b.extend(&chars);
        b.resize(b.len().next_multiple_of(4), 0);
        b.extend(le(&vec![dx; n as usize]));
        self.rec(84, &b)
    }

    /// The file: its header, the records, and the end.
    pub(crate) fn bytes(self) -> Vec<u8> {
        let eof = le(&[14, 20, 0, 16, 20]);
        let total = 108 + self.body.len() + eof.len();
        let mut d = le(&[1, 108, 0, 0, 99, 99, 0, 0, 2540, 2540, 0x464D_4520, 0x10000, total as i32, self.records as i32 + 2]);
        d.extend(16u16.to_le_bytes());
        d.extend(0u16.to_le_bytes());
        d.extend(le(&[0, 0, 0, 100, 100, 25, 25, 0, 0, 0, 25_400, 25_400]));
        d.extend(self.body);
        d.extend(eof);
        d
    }
}

/// What the player drew, a line an operation, with bounds in points to a tenth.
#[derive(Default)]
pub(super) struct Log {
    pub(super) ops: Vec<String>,
    pub(super) depth: i32,
}

pub(super) fn bounds(r: kurbo::Rect) -> String {
    // Rounded, then plus zero: no "-0.0".
    let v = |x: f64| (x * 10.0).round() / 10.0 + 0.0;
    format!("{:.1},{:.1},{:.1},{:.1}", v(r.x0), v(r.y0), v(r.x1), v(r.y1))
}

fn brush(b: &Brush) -> String {
    match b {
        Brush::Solid(c) => format!("#{:02x}{:02x}{:02x}", (c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8),
        Brush::Gradient(_) => "gradient".into(),
        Brush::Hatch { .. } => "hatch".into(),
        Brush::Bitmap { .. } => "bitmap".into(),
    }
}

impl Sink for Log {
    fn save(&mut self) {
        self.depth += 1;
        self.ops.push("save".into());
    }
    fn restore(&mut self) {
        self.depth -= 1;
        self.ops.push("restore".into());
    }
    fn transform(&mut self, _: Affine) {
        self.ops.push("transform".into());
    }
    fn clip_rect(&mut self, r: kurbo::Rect) {
        self.ops.push(format!("clip {}", bounds(r)));
    }
    fn clip_path(&mut self, p: &BezPath, even_odd: bool) {
        self.ops.push(format!("clip {}{}", bounds(p.bounding_box()), if even_odd { " even-odd" } else { "" }));
    }
    fn begin_layer(&mut self, _: f32) {}
    fn end_layer(&mut self) {}
    fn fill(&mut self, p: &BezPath, _: bool, b: &Brush) {
        self.ops.push(format!("fill {} {}", bounds(p.bounding_box()), brush(b)));
    }
    fn stroke(&mut self, p: &BezPath, s: &kurbo::Stroke, b: &Brush) {
        let dashed = if s.dash_pattern.is_empty() { "" } else { " dashed" };
        self.ops.push(format!("stroke {} {:.2}{dashed} {}", bounds(p.bounding_box()), s.width, brush(b)));
    }
    fn image(&mut self, _: &std::sync::Arc<Vec<u8>>, src: kurbo::Rect, to: Affine, _: f32, _: bool) {
        self.ops.push(format!("image {} into {}", bounds(src), bounds(to.transform_rect_bbox(kurbo::Rect::new(0.0, 0.0, 1.0, 1.0)))));
    }
    fn text(&mut self, t: &Text<'_>) -> f64 {
        let s: String = t.chars.iter().filter_map(|&c| char::from_u32(c)).collect();
        let at = t.transform * kurbo::Point::ZERO;
        let mut line = format!("text {s:?} {} at {:.1},{:.1}", t.family, at.x, at.y);
        if let Some(a) = t.advances {
            write!(line, " {a:?}").unwrap();
        }
        if t.extra != 0.0 || t.space_extra != 0.0 {
            write!(line, " extra {:.1} space {:.1}", t.extra, t.space_extra).unwrap();
        }
        if let Some(r) = t.rises {
            write!(line, " rises {r:?}").unwrap();
        }
        if t.underline {
            line.push_str(" underlined");
        }
        self.ops.push(line);
        10.0
    }
    /// Each character half the line's height across; an ascent of 0.8 of it and a descent of 0.2.
    fn measure(&mut self, t: &Text<'_>, advances: &mut Vec<f64>) -> Option<crate::render::svg::LineMetrics> {
        advances.extend(t.chars.iter().map(|_| t.height / 2.0));
        Some(crate::render::svg::LineMetrics { ascent: 0.8 * t.height, descent: 0.2 * t.height, leading: 0.0 })
    }
}

pub(super) fn log(e: Emf) -> Vec<String> {
    let mut l = Log::default();
    assert!(play(&mut l, &e.bytes()), "plays");
    assert_eq!(l.depth, 0, "saves and restores balance: {:?}", l.ops);
    l.ops
}

/// The operations that draw.
pub(super) fn drawn(ops: &[String]) -> Vec<&str> {
    ops.iter().map(String::as_str).filter(|o| *o != "save" && *o != "restore").collect()
}

#[test]
fn the_header_gives_the_frame_in_points() {
    let d = Emf::default().bytes();
    let h = Header::read(&d).expect("a header");
    assert_eq!((h.size.width.round(), h.size.height.round()), (72.0, 72.0));
    assert_eq!(crate::docx::image::image_type(&d), Some(("image/x-emf", "emf")));
    let (w, h, dx, dy) = crate::docx::image::image_info(&d).expect("a size");
    assert_eq!((w.round(), h.round(), dx, dy), (96.0, 96.0, 96.0, 96.0));
}

#[test]
fn a_rectangle_fills_and_strokes_in_points() {
    let ops = log(Emf::default().brush(1, 0x0000FF).r(43, &[10, 10, 60, 60]));
    assert_eq!(drawn(&ops), ["fill 7.2,7.2,43.2,43.2 #ff0000", "stroke 7.2,7.2,43.2,43.2 0.72 #000000"]);
}

#[test]
fn the_window_maps_onto_the_viewport() {
    // Anisotropic: a window 1000 units across on a viewport 100 pixels across, from (500, 500).
    let e = Emf::default().r(17, &[8]).r(9, &[1000, 1000]).r(11, &[100, 100]).r(10, &[500, 500]).r(43, &[500, 500, 1000, 1000]);
    assert_eq!(drawn(&log(e))[0], "fill 0.0,0.0,36.0,36.0 #ffffff");
    // Hundredths of a millimetre, y up: 12.7 mm is 50 device pixels.
    let e = Emf::default().r(17, &[3]).r(43, &[0, 0, 1270, -1270]);
    assert_eq!(drawn(&log(e))[0], "fill 0.0,0.0,36.0,36.0 #ffffff");
    // A world transform doubles and moves it.
    let xf: Vec<u8> = [2.0f32, 0.0, 0.0, 2.0, 10.0, 0.0].iter().flat_map(|v| v.to_le_bytes()).collect();
    let e = Emf::default().rec(35, &xf).r(43, &[0, 0, 25, 25]);
    assert_eq!(drawn(&log(e))[0], "fill 7.2,0.0,43.2,36.0 #ffffff");
}

#[test]
fn pens_dash_and_null_pens_draw_nothing() {
    // PS_DASH cosmetic; then PS_NULL.
    let e = Emf::default()
        .r(38, &[1, 1, 0, 0, 0xFF00])
        .r(37, &[1])
        .r(4, &[0, 0, 0, 0, 2, 0, 0, 50, 50])
        .r(37, &[NULL_PEN])
        .r(4, &[0, 0, 0, 0, 2, 0, 0, 50, 50]);
    assert_eq!(drawn(&log(e)), ["stroke 0.0,0.0,36.0,36.0 0.72 dashed #00ff00"]);
    // A geometric pen 10 units wide with square ends.
    let e = Emf::default().r(95, &[1, 0, 0, 0, 0, 0x10100, 10, 0, 0xFF, 0, 0]).r(37, &[1]).r(4, &[0, 0, 0, 0, 2, 0, 0, 50, 0]);
    assert_eq!(drawn(&log(e)), ["stroke 0.0,0.0,36.0,0.0 7.20 #ff0000"]);
}

#[test]
fn the_clip_saves_and_restores() {
    let e = Emf::default().r(33, &[]).r(30, &[0, 0, 50, 50]).r(43, &[0, 0, 100, 100]).r(34, &[-1]).r(43, &[0, 0, 100, 100]);
    let ops = log(e);
    let clip = ops.iter().position(|o| o == "clip 0.0,0.0,36.0,36.0 even-odd").expect("clipped");
    let fills: Vec<usize> = ops.iter().enumerate().filter(|(_, o)| o.starts_with("fill")).map(|(i, _)| i).collect();
    assert!(clip < fills[0], "{ops:?}");
    // Restoring drops the clip: the sink restores past it before the second rectangle.
    assert!(ops[fills[0]..fills[1]].iter().any(|o| o == "restore"), "{ops:?}");
    assert!(!ops[fills[0]..fills[1]].iter().any(|o| o.starts_with("clip")), "{ops:?}");
}

#[test]
fn an_excluded_rect_is_cut_from_the_clip() {
    let ops = log(Emf::default().r(29, &[10, 10, 20, 20]).r(43, &[0, 0, 50, 50]));
    assert!(ops.iter().any(|o| o.starts_with("clip") && o.ends_with("even-odd")), "{ops:?}");
    // A region selected as the clip, then reset by an empty one.
    let rgn = le(&[32, 1, 1, 16, 0, 0, 50, 50, 0, 0, 50, 50]);
    let mut b = le(&[rgn.len() as i32, 5]);
    b.extend(&rgn);
    let ops = log(Emf::default().rec(75, &b).r(43, &[0, 0, 10, 10]).r(75, &[0, 5]).r(43, &[0, 0, 10, 10]));
    assert_eq!(ops.iter().filter(|o| *o == "clip 0.0,0.0,36.0,36.0 even-odd").count(), 1, "{ops:?}");
}

#[test]
fn paths_fill_as_one_figure() {
    let e = Emf::default().r(59, &[]).r(27, &[0, 0]).r(54, &[50, 0]).r(54, &[50, 50]).r(61, &[]).r(60, &[]).r(62, &[0, 0, 0, 0]);
    assert_eq!(drawn(&log(e)), ["fill 0.0,0.0,36.0,36.0 #ffffff"]);
    // A widened path fills as the pen's stroke.
    let e = Emf::default().r(59, &[]).r(27, &[0, 0]).r(54, &[50, 0]).r(60, &[]).r(66, &[]).r(62, &[0, 0, 0, 0]);
    assert_eq!(drawn(&log(e)), ["stroke 0.0,0.0,36.0,0.0 0.72 #ffffff"]);
}

#[test]
fn shapes_are_their_outlines() {
    let e = Emf::default().r(37, &[NULL_PEN]).r(42, &[0, 0, 50, 25]).r(44, &[0, 0, 50, 50, 20, 10]).r(47, &[0, 0, 50, 50, 50, 25, 25, 0]);
    let ops = log(e);
    assert_eq!(drawn(&ops)[0], "fill 0.0,0.0,36.0,18.0 #ffffff");
    assert_eq!(drawn(&ops)[1], "fill 0.0,0.0,36.0,36.0 #ffffff");
    // The pie from the right anticlockwise to the top: the top right quarter.
    assert_eq!(drawn(&ops)[2], "fill 18.0,0.0,36.0,18.0 #ffffff");
}

#[test]
fn text_places_by_its_spacing() {
    let mut font = le(&[1, -20, 0, 0, 0, 400, 0, 0]);
    font.extend(wide("Arial"));
    font.resize(12 + 92, 0);
    let ops = log(Emf::default().rec(82, &font).r(37, &[1]).r(22, &[24]).text(10, 50, "Hi", 20));
    assert_eq!(drawn(&ops), ["text \"Hi\" Arial at 7.2,36.0 [20.0, 20.0]"]);
}

#[test]
fn bitmaps_draw_their_pixels() {
    // A 2 × 1 24-bit DIB, red then blue, stretched over the picture's left half.
    let mut bmi = le(&[40, 2, 1]);
    bmi.extend(1u16.to_le_bytes());
    bmi.extend(24u16.to_le_bytes());
    bmi.extend(le(&[0, 8, 0, 0, 0, 0]));
    let bits = [0, 0, 255, 255, 0, 0, 0, 0];
    let mut b = le(&[0, 0, 0, 0, 0, 0, 0, 0, 2, 1, 80, 40, 120, 8, 0, 0x00CC_0020, 50, 100]);
    b.extend(&bmi);
    b.extend(bits);
    let d = Emf::default().rec(81, &b).bytes();
    let px = crate::render::graphics::Graphics::metafile_pixels(&d, 100, 100, None).expect("draws");
    assert_eq!((px[50 * 100 + 5], px[50 * 100 + 45], px[50 * 100 + 75]), ([255, 0, 0, 255], [0, 0, 255, 255], [255; 4]));
    // AlphaBlend of one premultiplied pixel, red at half alpha, over the whole picture.
    let mut b = le(&[0, 0, 0, 0, 0, 0, 100, 100]);
    b.extend([0, 0, 255, 1]);
    b.extend(le(&[0, 0]));
    b.extend([1.0f32, 0.0, 0.0, 1.0, 0.0, 0.0].iter().flat_map(|v| v.to_le_bytes()));
    b.extend(le(&[0, 0, 108, 40, 148, 4, 1, 1]));
    b.extend(le(&[40, 1, 1, 0x0020_0001, 0, 4, 0, 0, 0, 0]));
    b.extend([0, 0, 128, 128]);
    let px = crate::render::graphics::Graphics::metafile_pixels(&Emf::default().rec(114, &b).bytes(), 10, 10, None).expect("draws");
    let p = px[55];
    assert!(p[0] == 255 && (120..136).contains(&p[1]) && p[1] == p[2], "half-transparent red over white: {p:?}");
}

#[test]
fn gradients_and_hatches_shade() {
    // A rect shaded red to blue across.
    let mut b = le(&[0, 0, 0, 0, 2, 1, 0]);
    b.extend(le(&[0, 0]));
    b.extend([0, 0xFF, 0, 0, 0, 0, 0, 0]);
    b.extend(le(&[100, 100]));
    b.extend([0, 0, 0, 0, 0, 0xFF, 0, 0]);
    b.extend(le(&[0, 1]));
    let d = Emf::default().rec(118, &b).bytes();
    let px = crate::render::graphics::Graphics::metafile_pixels(&d, 100, 100, None).expect("draws");
    let (left, right) = (px[50 * 100 + 2], px[50 * 100 + 97]);
    assert!(left[0] > 240 && left[2] < 15 && right[2] > 240 && right[0] < 15, "{left:?} {right:?}");
    // A cross hatch in green on its transparent background.
    let d = Emf::default().r(18, &[1]).r(39, &[1, 2, 0xFF00, 4]).r(37, &[1]).r(37, &[NULL_PEN]).r(43, &[0, 0, 100, 100]).bytes();
    let px = crate::render::graphics::Graphics::metafile_pixels(&d, 100, 100, None).expect("draws");
    assert!(px.iter().any(|p| p[1] > 200 && p[0] < 60) && px.iter().any(|p| p == &[255; 4]), "lines and gaps");
}

#[test]
fn every_drawing_record_draws() {
    let blit = {
        let mut b = le(&[0, 0, 0, 0, 0, 0, 50, 50, 0x00F0_0021, 0, 0]);
        b.extend([1.0f32, 0.0, 0.0, 1.0, 0.0, 0.0].iter().flat_map(|v| v.to_le_bytes()));
        b.extend(le(&[0, 0, 0, 0, 0, 0]));
        b
    };
    let dib = {
        let mut b = le(&[0, 0, 0, 0, 50, 50, 0, 0, 1, 1, 76, 40, 116, 4, 0, 0, 1]);
        b.extend(le(&[40, 1, 1, 0x0018_0001, 0, 4, 0, 0, 0, 0]));
        b.extend([0, 0, 255, 0]);
        b
    };
    let poly_text = {
        let mut b = le(&[0, 0, 0, 0, 1, 0, 0, 1, 10, 20, 2, 80, 0, 0, 0, 0, 0, 0]);
        b.extend(wide("ab"));
        b
    };
    let region = {
        let rgn = le(&[32, 1, 1, 16, 0, 0, 25, 25, 0, 0, 25, 25]);
        let mut b = le(&[0, 0, 0, 0, rgn.len() as i32, 1]);
        b.extend(&rgn);
        b
    };
    let e = Emf::default()
        .brush(1, 0x0000FF)
        .r(37, &[NULL_PEN])
        .r(8, &[0, 0, 0, 0, 2, 6, 3, 3, 0, 0, 25, 0, 0, 25, 50, 50, 25, 50, 50, 25])
        .r(37, &[NULL_PEN - 1])
        .rec(56, &[le(&[0, 0, 0, 0, 3, 0, 0, 50, 0, 50, 50]), vec![6, 2, 3]].concat())
        .r(37, &[NULL_PEN])
        .rec(76, &blit)
        .rec(80, &dib)
        .rec(97, &poly_text)
        .r(46, &[0, 0, 50, 50, 50, 25, 0, 25])
        .rec(71, &region);
    let ops = log(e);
    assert_eq!(
        drawn(&ops),
        [
            "fill 0.0,0.0,36.0,36.0 #ff0000",
            "stroke 0.0,0.0,36.0,36.0 0.72 #000000",
            "fill 0.0,0.0,36.0,36.0 #ff0000",
            "image 0.0,0.0,1.0,1.0 into 36.0,36.0,36.7,36.7",
            "text \"ab\" Arial at 7.2,14.4",
            "fill 0.0,0.0,36.0,18.0 #ff0000",
            "fill 0.0,0.0,18.0,18.0 #ff0000",
        ]
    );
}

// ---------------- raster operations, regions, palettes and masks ----------------

/// The picture drawn at 100 × 100 pixels (a device pixel each) over white.
pub(super) fn pixels(e: Emf) -> Vec<[u8; 4]> {
    crate::render::graphics::Graphics::metafile_pixels(&e.bytes(), 100, 100, None).expect("draws")
}

pub(super) fn at(px: &[[u8; 4]], x: usize, y: usize) -> [u8; 3] {
    let p = px[y * 100 + x];
    [p[0], p[1], p[2]]
}

pub(super) const RED: [u8; 3] = [255, 0, 0];
pub(super) const GREEN: [u8; 3] = [0, 255, 0];
pub(super) const BLUE: [u8; 3] = [0, 0, 255];
pub(super) const BLACK: [u8; 3] = [0; 3];
pub(super) const WHITE: [u8; 3] = [255; 3];

/// A BITMAPINFOHEADER (bottom up) and its colour table, and its rows each padded to four bytes.
pub(super) fn dib(w: i32, h: i32, bpp: u16, table: &[u8], rows: &[&[u8]], used: i32) -> (Vec<u8>, Vec<u8>) {
    let mut bmi = le(&[40, w, h]);
    bmi.extend(1u16.to_le_bytes());
    bmi.extend(bpp.to_le_bytes());
    let bits: Vec<u8> = rows.iter().flat_map(|r| r.iter().copied().chain(std::iter::repeat_n(0, r.len().next_multiple_of(4) - r.len()))).collect();
    bmi.extend(le(&[0, bits.len() as i32, 0, 0, used, 0]));
    bmi.extend(table);
    (bmi, bits)
}

/// EMR_STRETCHDIBITS of a DIB over `dst` (x, y, w, h) by `rop`, with colour usage `usage`.
fn stretch(e: Emf, (bmi, bits): (Vec<u8>, Vec<u8>), dst: [i32; 4], src: (i32, i32), rop: u32, usage: i32) -> Emf {
    let mut b = le(&[0, 0, 0, 0, dst[0], dst[1], 0, 0, src.0, src.1, 80, bmi.len() as i32, 80 + bmi.len() as i32, bits.len() as i32, usage]);
    b.extend(rop.to_le_bytes());
    b.extend(le(&[dst[2], dst[3]]));
    b.extend(&bmi);
    b.extend(&bits);
    e.rec(81, &b)
}

/// A black left half on the white ground.
fn half() -> Emf {
    Emf::default().r(37, &[NULL_PEN]).brush(9, 0).r(43, &[0, 0, 50, 100])
}

#[test]
fn binary_raster_operations_read_what_is_under() {
    // R2_XORPEN: white over black and white.
    let px = pixels(half().r(20, &[7]).brush(2, 0xFF_FFFF).r(43, &[25, 0, 75, 100]));
    assert_eq!([at(&px, 10, 50), at(&px, 40, 50), at(&px, 60, 50), at(&px, 90, 50)], [BLACK, WHITE, BLACK, WHITE]);
    // R2_NOT inverts whatever the pen; R2_NOP draws nothing.
    let px = pixels(half().r(20, &[6]).r(43, &[0, 0, 100, 50]).r(20, &[11]).brush(3, 0xFF).r(43, &[0, 50, 100, 100]));
    assert_eq!([at(&px, 10, 25), at(&px, 90, 25), at(&px, 10, 75), at(&px, 90, 75)], [WHITE, BLACK, BLACK, WHITE]);
}

#[test]
fn ternary_raster_operations_combine_pattern_source_and_destination() {
    let red = || dib(1, 1, 24, &[], &[&[0, 0, 255]], 0);
    // DSTINVERT, no source: the top half inverted. SRCINVERT of red: the bottom half's black red, its white cyan.
    let mut blit = le(&[0, 0, 0, 0, 0, 0, 100, 50, 0x0055_0009, 0, 0]);
    blit.extend([1.0f32, 0.0, 0.0, 1.0, 0.0, 0.0].iter().flat_map(|v| v.to_le_bytes()));
    blit.extend(le(&[0, 0, 0, 0, 0, 0]));
    let px = pixels(stretch(half().rec(76, &blit), red(), [0, 50, 100, 50], (1, 1), 0x0066_0046, 0));
    assert_eq!([at(&px, 10, 25), at(&px, 90, 25), at(&px, 10, 75), at(&px, 90, 75)], [WHITE, BLACK, RED, [0, 255, 255]]);
    // 0xB8, the source choosing: where it is white the destination, where black the pattern (a green brush).
    let source = dib(2, 1, 24, &[], &[&[255, 255, 255, 0, 0, 0]], 0);
    let px = pixels(stretch(half().brush(2, 0xFF00), source, [0, 0, 100, 100], (2, 1), 0x00B8_074A, 0));
    assert_eq!([at(&px, 10, 50), at(&px, 40, 50), at(&px, 60, 50), at(&px, 90, 50)], [BLACK, BLACK, GREEN, GREEN]);
    // MERGECOPY (pattern and source): green and red is black, green and white green; then PATINVERT.
    let source = dib(2, 1, 24, &[], &[&[0, 0, 255, 255, 255, 255]], 0);
    let px = pixels(stretch(Emf::default().brush(2, 0xFF00), source, [0, 0, 100, 100], (2, 1), 0x00C0_00CA, 0));
    assert_eq!([at(&px, 25, 50), at(&px, 75, 50)], [BLACK, GREEN]);
}

#[test]
fn inverted_and_framed_regions() {
    let rgn = |rects: &[[i32; 4]]| {
        let mut d = le(&[32, 1, rects.len() as i32, 16 * rects.len() as i32, 0, 0, 100, 100]);
        rects.iter().for_each(|r| d.extend(le(r)));
        d
    };
    let r = rgn(&[[0, 0, 100, 100]]);
    let mut b = le(&[0, 0, 0, 0, r.len() as i32]);
    b.extend(&r);
    let px = pixels(half().rec(73, &b));
    assert_eq!([at(&px, 10, 50), at(&px, 90, 50)], [WHITE, BLACK]);
    // Two touching rects framed 4 wide: the frame follows the region's edge, not the seam between them.
    let r = rgn(&[[0, 0, 50, 50], [50, 0, 100, 50]]);
    let mut b = le(&[0, 0, 0, 0, r.len() as i32, 1, 4, 4]);
    b.extend(&r);
    let px = pixels(Emf::default().r(39, &[1, 0, 0xFF, 0]).rec(72, &b));
    assert_eq!([at(&px, 2, 25), at(&px, 25, 2), at(&px, 97, 25), at(&px, 25, 47)], [RED; 4]);
    assert_eq!([at(&px, 50, 25), at(&px, 25, 25), at(&px, 25, 60)], [WHITE; 3]);
}

#[test]
fn clips_combine_by_union_and_exclusive_or() {
    let select = |e: Emf, r: [i32; 4], mode: i32| {
        let rgn = [le(&[32, 1, 1, 16]), le(&r), le(&r)].concat();
        e.rec(75, &[le(&[rgn.len() as i32, mode]), rgn].concat())
    };
    let fill = |e: Emf| e.r(37, &[NULL_PEN]).brush(1, 0xFF).r(43, &[0, 0, 100, 100]);
    let base = || select(Emf::default(), [0, 0, 60, 100], 5);
    let px = pixels(fill(select(base(), [40, 0, 100, 50], 2)));
    assert_eq!([at(&px, 10, 90), at(&px, 50, 25), at(&px, 90, 10), at(&px, 90, 90)], [RED, RED, RED, WHITE]);
    let px = pixels(fill(select(base(), [40, 0, 100, 50], 3)));
    assert_eq!([at(&px, 10, 90), at(&px, 50, 25), at(&px, 90, 10), at(&px, 90, 90)], [RED, WHITE, RED, WHITE]);
    // An intersection, then a union with it.
    let px = pixels(fill(select(base().r(30, &[0, 0, 100, 50]), [80, 80, 100, 100], 2)));
    assert_eq!([at(&px, 10, 10), at(&px, 10, 90), at(&px, 90, 90), at(&px, 90, 10)], [RED, WHITE, RED, WHITE]);
}

#[test]
fn triangles_shade_between_their_corners() {
    let vertex = |x: i32, y: i32, c: [u16; 3]| [le(&[x, y]), [c[0], c[1], c[2], 0].iter().flat_map(|v| v.to_le_bytes()).collect()].concat();
    let mut b = le(&[0, 0, 0, 0, 3, 1, 2]);
    b.extend(vertex(0, 0, [0xFF00, 0, 0]));
    b.extend(vertex(100, 0, [0, 0xFF00, 0]));
    b.extend(vertex(0, 100, [0, 0, 0xFF00]));
    b.extend(le(&[0, 1, 2]));
    let px = pixels(Emf::default().rec(118, &b));
    // At a pixel's centre (x, y) the colour is red 1 - x - y, green x, blue y (in hundredths).
    for (x, y) in [(10, 10), (30, 30), (60, 20), (20, 70)] {
        let (fx, fy) = ((x as f64 + 0.5) / 100.0, (y as f64 + 0.5) / 100.0);
        let want = [1.0 - fx - fy, fx, fy].map(|v| (v * 255.0) as i32);
        let got = at(&px, x, y).map(i32::from);
        assert!(want.iter().zip(got).all(|(w, g)| (w - g).abs() <= 3), "at {x}, {y}: {got:?} not {want:?}");
    }
    assert_eq!(at(&px, 90, 90), WHITE);
}

/// The gates' sans stand-in, whose glyphs are boxes.
pub(super) fn stand_in() -> crate::font::Fonts {
    let mut fonts = crate::font::Fonts::default();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/fonts/stand-in-sans.ttf");
    fonts.add_bytes(std::fs::read(path).expect("the stand-in")).expect("a face");
    fonts
}

#[test]
fn text_in_a_path_bracket_fills_as_its_outlines() {
    let mut font = le(&[1, -40, 0, 0, 0, 400, 0, 0]);
    font.resize(12 + 92, 0);
    let text = |e: Emf| e.rec(82, &font).r(37, &[1]).r(22, &[24]).r(18, &[1]).text(10, 60, "HI", 30);
    let fonts = stand_in();
    let draw = |e: Emf| crate::render::graphics::Graphics::metafile_pixels(&e.bytes(), 100, 100, Some(&fonts)).expect("draws");
    let dark = |px: &[[u8; 4]]| px.iter().filter(|p| p[0] < 128).count();
    let direct = draw(text(Emf::default()));
    // The outlines filled with a black brush, no pen.
    let path = draw(text(Emf::default().r(37, &[NULL_PEN]).brush(5, 0).r(59, &[])).r(60, &[]).r(62, &[0, 0, 0, 0]));
    assert!(dark(&direct) > 100, "the text draws");
    assert!(dark(&path).abs_diff(dark(&direct)) < dark(&direct) / 10, "{} against {}", dark(&path), dark(&direct));
}

#[test]
fn monochrome_brushes_and_hatched_pens() {
    // An 8 × 8 one-bit checker as CreateMonoBrush makes it: drawn in the text colour (0 bits) and background (1).
    let row = |k: usize| vec![if k.is_multiple_of(2) { 0xAA } else { 0x55 }];
    let rows: Vec<Vec<u8>> = (0..8).map(row).collect();
    let (bmi, bits) = dib(8, 8, 1, &[0, 0, 0, 0, 255, 255, 255, 0], &rows.iter().map(Vec::as_slice).collect::<Vec<_>>(), 0);
    let mut b = le(&[1, 0, 32, bmi.len() as i32, 32 + bmi.len() as i32, bits.len() as i32]);
    b.extend(&bmi);
    b.extend(&bits);
    let e = Emf::default().r(24, &[0xFF]).r(25, &[0xFF_0000]).rec(93, &b).r(37, &[1]).r(37, &[NULL_PEN]).r(43, &[0, 0, 100, 100]);
    let px = pixels(e);
    assert!(px.iter().all(|p| [p[0], p[1], p[2]] == RED || [p[0], p[1], p[2]] == BLUE), "only the text and background colours");
    // A geometric pen 40 wide stroking a cross hatch, green on its blue background.
    let e =
        Emf::default().r(25, &[0xFF_0000]).r(95, &[2, 0, 0, 0, 0, 0x10200, 40, 2, 0xFF00, 4, 0]).r(37, &[2]).r(4, &[0, 0, 0, 0, 2, 0, 50, 100, 50]);
    let px = pixels(e);
    let band: Vec<[u8; 3]> = (32..68).map(|y| at(&px, 50, y)).collect();
    assert!(band.contains(&GREEN) && band.contains(&BLUE) && at(&px, 50, 10) == WHITE, "{band:?}");
}

#[test]
fn a_strip_of_scans_draws_where_it_belongs() {
    // Four rows tall, the bits holding scans 1 and 2 from the bottom (red, then green): rows 2 and 1 from the top.
    let (bmi, bits) = dib(1, 4, 24, &[], &[&[0, 0, 255], &[0, 255, 0]], 0);
    let mut b = le(&[0, 0, 0, 0, 0, 0, 0, 0, 1, 4, 76, bmi.len() as i32, 76 + bmi.len() as i32, bits.len() as i32, 0, 1, 2]);
    b.extend(&bmi);
    b.extend(&bits);
    let px = pixels(Emf::default().r(17, &[8]).r(9, &[4, 4]).r(11, &[100, 100]).rec(80, &b));
    assert_eq!([at(&px, 10, 10), at(&px, 10, 35), at(&px, 10, 60), at(&px, 10, 90)], [WHITE, GREEN, RED, WHITE]);
}

#[test]
fn spacing_pairs_move_characters_up() {
    let mut b = le(&[0, 0, 0, 0, 1, 0, 0, 10, 50, 2, 76, 0x2000, 0, 0, 0, 0, 80]);
    b.extend(wide("ab"));
    b.extend(le(&[20, 5, 20, -5]));
    assert_eq!(drawn(&log(Emf::default().rec(84, &b))), ["text \"ab\" Arial at 7.2,36.0 [20.0, 20.0] rises [-5.0, 5.0]"]);
}

#[test]
fn palette_colours_by_index() {
    // A palette of red and green selected; an 8-bit DIB whose table indexes it (green, red); a brush by index.
    let e = Emf::default().rec(49, &[le(&[1]), vec![0, 3, 2, 0], vec![255, 0, 0, 0, 0, 255, 0, 0]].concat()).r(48, &[1]);
    let table = [1u8, 0, 0, 0];
    let e = stretch(e, dib(2, 1, 8, &table, &[&[0, 1]], 2), [0, 0, 100, 50], (2, 1), 0x00CC_0020, 1);
    let px = pixels(e.r(37, &[NULL_PEN]).brush(2, 0x0100_0001).r(43, &[0, 50, 100, 100]));
    assert_eq!([at(&px, 25, 25), at(&px, 75, 25), at(&px, 50, 75)], [GREEN, RED, GREEN]);
}

#[test]
fn keyed_masked_and_parallelogram_blits() {
    let xform: Vec<u8> = [1.0f32, 0.0, 0.0, 1.0, 0.0, 0.0].iter().flat_map(|v| v.to_le_bytes()).collect();
    let (bmi, bits) = dib(2, 1, 24, &[], &[&[0, 0, 255, 255, 255, 255]], 0);
    let wide = || Emf::default().r(17, &[8]).r(9, &[2, 1]).r(11, &[100, 100]);
    // TransparentBlt keying out white over the black left half: red, then the ground.
    let mut b = le(&[0, 0, 0, 0, 0, 0, 2, 1, 0xFF_FFFF, 0, 0]);
    b.extend(&xform);
    b.extend(le(&[0, 0, 108, bmi.len() as i32, 108 + bmi.len() as i32, bits.len() as i32, 2, 1]));
    b.extend(&bmi);
    b.extend(&bits);
    let px = pixels(wide().r(37, &[NULL_PEN]).brush(9, 0).r(43, &[1, 0, 2, 1]).rec(116, &b));
    assert_eq!([at(&px, 25, 50), at(&px, 75, 50)], [RED, BLACK]);
    // MaskBlt copying where the mask's first bit is set, leaving the rest.
    let (mbmi, mbits) = dib(2, 1, 1, &[0, 0, 0, 0, 255, 255, 255, 0], &[&[0x80]], 0);
    let mut b = le(&[0, 0, 0, 0, 0, 0, 2, 1, 0xAACC_0020u32.cast_signed(), 0, 0]);
    b.extend(&xform);
    let (s, m) = (128, 128 + bmi.len() as i32 + bits.len() as i32);
    b.extend(le(&[0, 0, s, bmi.len() as i32, s + bmi.len() as i32, bits.len() as i32, 0, 0, 0]));
    b.extend(le(&[m, mbmi.len() as i32, m + mbmi.len() as i32, mbits.len() as i32]));
    b.extend(&bmi);
    b.extend(&bits);
    b.extend(&mbmi);
    b.extend(&mbits);
    let px = pixels(wide().r(37, &[NULL_PEN]).brush(9, 0).r(43, &[0, 0, 2, 1]).rec(78, &b));
    assert_eq!([at(&px, 25, 50), at(&px, 75, 50)], [RED, BLACK]);
    // PlgBlt of the red pixel onto a diamond.
    let (rbmi, rbits) = dib(1, 1, 24, &[], &[&[0, 0, 255]], 0);
    let mut b = le(&[0, 0, 0, 0, 50, 0, 100, 50, 0, 50, 0, 0, 1, 1]);
    b.extend(&xform);
    b.extend(le(&[0, 0, 140, rbmi.len() as i32, 140 + rbmi.len() as i32, rbits.len() as i32, 0, 0, 0, 0, 0, 0, 0]));
    b.extend(&rbmi);
    b.extend(&rbits);
    let px = pixels(Emf::default().rec(79, &b));
    assert_eq!([at(&px, 50, 50), at(&px, 50, 10), at(&px, 5, 5), at(&px, 95, 95)], [RED, RED, WHITE, WHITE]);
}

// ---------------- hostile files ----------------

/// Runs `f`, which must take less than `secs` seconds. Untimed under `cargo llvm-cov` (it sets `LLVM_PROFILE_FILE`),
/// whose counters slow the engine several times over; the counted budgets still hold there.
pub(super) fn within<T>(secs: u64, f: impl FnOnce() -> T) -> T {
    let start = Instant::now();
    let out = f();
    let took = start.elapsed();
    assert!(took < Duration::from_secs(secs) || std::env::var_os("LLVM_PROFILE_FILE").is_some(), "took {took:?}");
    out
}

/// Plays `d`, which must finish quickly and leave the sink balanced.
pub(super) fn survives(d: &[u8]) -> Vec<String> {
    let mut l = Log::default();
    within(5, || play(&mut l, d));
    assert_eq!(l.depth, 0);
    l.ops
}

#[test]
fn a_truncated_header_isnt_an_emf() {
    let d = Emf::default().r(43, &[0, 0, 50, 50]).bytes();
    for n in [0, 4, 44, 60, 87, 100] {
        assert!(Header::read(&d[..n]).is_none(), "{n} bytes");
        assert!(!play(&mut Log::default(), &d[..n]));
    }
    // A header claiming more bytes than there are.
    let mut d = d;
    d[4..8].copy_from_slice(&10_000u32.to_le_bytes());
    assert!(Header::read(&d).is_none());
}

#[test]
fn a_zero_size_record_stops_the_file() {
    let mut d = Emf::default().r(43, &[0, 0, 50, 50]).bytes();
    // The rectangle's size, zeroed; then one not a multiple of four, and one past the end.
    for size in [0u32, 7, 30, 1 << 31] {
        d[112..116].copy_from_slice(&size.to_le_bytes());
        assert!(drawn(&survives(&d)).is_empty(), "size {size}");
    }
}

#[test]
fn counts_of_two_to_the_thirty_two_read_nothing() {
    let max = -1;
    for e in [
        Emf::default().r(4, &[0, 0, 0, 0, max, 0, 0]),
        Emf::default().r(8, &[0, 0, 0, 0, max, max, 1, 0, 0]),
        Emf::default().r(56, &[0, 0, 0, 0, max, 0, 0, 6]),
        Emf::default().r(118, &[0, 0, 0, 0, max, max, 0]),
        Emf::default().r(97, &[0, 0, 0, 0, 1, 0, 0, max]),
        Emf::default().r(84, &[0, 0, 0, 0, 1, 0, 0, 0, 0, max, 76, 0, 0, 0, 0, 0, 0]),
        Emf::default().r(75, &[max, 5, 32, 1, max, 0]),
        Emf::default().r(81, &[0, 0, 0, 0, 0, 0, 0, 0, 2, 1, 80, max, 120, max, 0, 0x00CC_0020, 50, 100]),
    ] {
        assert!(drawn(&survives(&e.bytes())).is_empty());
    }
}

#[test]
fn a_bitmap_too_big_isnt_drawn() {
    // A 65536 × 65536 header with 8 bytes of bits.
    let mut b = le(&[0, 0, 0, 0, 0, 0, 0, 0, 2, 1, 80, 40, 120, 8, 0, 0x00CC_0020, 50, 100]);
    b.extend(le(&[40, 65536, 65536, 0x0018_0001, 0, 8, 0, 0, 0, 0]));
    b.extend([0; 8]);
    assert!(drawn(&survives(&Emf::default().rec(81, &b).bytes())).is_empty());
}

#[test]
fn objects_naming_themselves_or_nothing() {
    // Created at handle 0 (the metafile itself) and past the table, selected, deleted while selected.
    let e = Emf::default()
        .r(39, &[0, 0, 0xFF, 0])
        .r(37, &[0])
        .r(39, &[100_000, 0, 0xFF, 0])
        .r(37, &[100_000])
        .r(37, &[-1])
        .r(39, &[1, 0, 0xFF, 0])
        .r(37, &[1])
        .r(40, &[1])
        .r(37, &[1])
        .r(40, &[0])
        .r(43, &[0, 0, 50, 50]);
    // The brush deleted while selected still draws.
    assert_eq!(drawn(&survives(&e.bytes()))[0], "fill 0.0,0.0,36.0,36.0 #ff0000");
}

#[test]
fn a_save_storm_keeps_a_bounded_stack() {
    let mut e = Emf::default();
    for _ in 0..200_000 {
        e = e.r(33, &[]);
    }
    let e = e.r(34, &[i32::MIN]).r(34, &[1]).r(34, &[i32::MAX]).r(30, &[0, 0, 50, 50]);
    let mut e = e;
    for _ in 0..10_000 {
        e = e.r(30, &[0, 0, 50, 50]);
    }
    let ops = survives(&e.r(43, &[0, 0, 50, 50]).bytes());
    assert!(ops.iter().filter(|o| o.starts_with("clip")).count() <= super::MAX_CLIPS, "the clip stays bounded");
}

#[test]
fn degenerate_transforms_and_huge_coordinates() {
    let xf = |v: [f32; 6]| -> Vec<u8> { v.iter().flat_map(|x| x.to_le_bytes()).collect() };
    let mut e = Emf::default()
        .rec(35, &xf([0.0; 6]))
        .rec(35, &xf([f32::NAN, 0.0, 0.0, 1.0, 0.0, 0.0]))
        .rec(35, &xf([1e30, 0.0, 0.0, 1e30, 0.0, 0.0]))
        .r(17, &[8])
        .r(9, &[0, 0])
        .r(11, &[i32::MAX, i32::MIN]);
    // A dashed pen along an enormous line.
    e = e.r(38, &[1, 2, 0, 0, 0]).r(37, &[1]).r(4, &[0, 0, 0, 0, 2, i32::MIN, i32::MIN, i32::MAX, i32::MAX]);
    e = e.r(42, &[i32::MIN, i32::MIN, i32::MAX, i32::MAX]).r(41, &[0, 0, -1, 0, 0x7F80_0000]);
    let d = e.bytes();
    survives(&d);
    within(5, || crate::render::graphics::Graphics::metafile_pixels(&d, 50, 50, None));
}

#[test]
fn the_fixture_reads_without_undecodable_pictures() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/render/metafile-emf.docx");
    let d = crate::docx::read(&std::fs::read(path).expect("fixture")).expect("reads");
    assert!(d.images.iter().all(|(_, i)| Header::read(i).is_some()), "every picture an EMF");
    assert!(!d.diagnostics.warnings.iter().any(|w| w.code == crate::api::warning::WarningCode::ImageUndecodable));
}

/// What a sink is sent, counted: path elements filled, stroked and clipped by; each mask's once (a sink records a
/// mask the first time and places it after) and one a draw under it; the bitmaps' bytes, each once.
#[derive(Default)]
pub(super) struct Count {
    pub(super) elements: u64,
    // Held, so a freed one's address can't be taken by the next and hide it.
    masks: HashMap<usize, std::rc::Rc<[crate::render::svg::MaskStep]>>,
    pub(super) bitmaps: HashMap<usize, std::sync::Arc<Vec<u8>>>,
    pub(super) bitmap_bytes: u64,
    pub(super) depth: i32,
    /// Every call the sink takes.
    pub(super) calls: u64,
}

impl Count {
    fn bitmap(&mut self, b: &std::sync::Arc<Vec<u8>>) {
        if self.bitmaps.insert(std::sync::Arc::as_ptr(b) as usize, b.clone()).is_none() {
            self.bitmap_bytes += b.len() as u64;
        }
    }
}

impl Sink for Count {
    fn save(&mut self) {
        self.depth += 1;
        self.calls += 1;
    }
    fn restore(&mut self) {
        self.depth -= 1;
        self.calls += 1;
    }
    fn transform(&mut self, _: Affine) {
        self.calls += 1;
    }
    fn clip_rect(&mut self, _: kurbo::Rect) {
        self.elements += 5;
        self.calls += 1;
    }
    fn clip_path(&mut self, p: &BezPath, _: bool) {
        self.elements += p.elements().len() as u64;
        self.calls += 1;
    }
    fn begin_layer(&mut self, _: f32) {
        self.calls += 1;
    }
    fn end_layer(&mut self) {
        self.calls += 1;
    }
    fn fill(&mut self, p: &BezPath, _: bool, b: &Brush) {
        self.calls += 1;
        self.elements += p.elements().len() as u64;
        if let Brush::Bitmap { bitmap, .. } = b {
            self.bitmap(bitmap);
        }
    }
    fn stroke(&mut self, p: &BezPath, _: &kurbo::Stroke, _: &Brush) {
        self.calls += 1;
        self.elements += p.elements().len() as u64;
    }
    fn image(&mut self, b: &std::sync::Arc<Vec<u8>>, _: kurbo::Rect, _: Affine, _: f32, _: bool) {
        self.calls += 1;
        self.bitmap(b);
    }
    fn end_masked(&mut self, mask: &std::rc::Rc<[crate::render::svg::MaskStep]>) {
        use crate::render::svg::MaskStep;
        self.calls += 1;
        self.elements += 1;
        if self.masks.insert(std::rc::Rc::as_ptr(mask) as *const () as usize, mask.clone()).is_none() {
            for step in mask.iter() {
                if let MaskStep::Add(p, _) | MaskStep::Remove(p, _) | MaskStep::Toggle(p, _) | MaskStep::Clip(p, _) = step {
                    self.elements += p.elements().len() as u64;
                }
            }
        }
    }
}

/// Plays `d` into a counting sink, which must not hang; what it counts is what the budgets bound.
pub(super) fn counted(d: &[u8]) -> Count {
    let mut c = Count::default();
    within(60, || play(&mut c, d));
    assert_eq!(c.depth, 0);
    c
}

/// EMR_EXTSELECTCLIPRGN of `n` one-pixel rects down a column, by `mode`.
pub(super) fn many_rects(e: Emf, n: i32, mode: i32) -> Emf {
    let mut rgn = le(&[32, 1, n, 16 * n, 0, 0, 1, n]);
    for i in 0..n {
        rgn.extend(le(&[0, i, 1, i + 1]));
    }
    e.rec(75, &[le(&[rgn.len() as i32, mode]), rgn].concat())
}

#[test]
fn a_big_clip_is_sent_once_and_counted() {
    // A union with 60 000 rects, then 20 000 lines under it: the mask is recorded once.
    let mut e = many_rects(Emf::default().r(30, &[0, 0, 100, 100]), 60_000, 2);
    for i in 0..20_000 {
        e = e.r(54, &[i % 100, 50]);
    }
    let c = counted(&e.bytes());
    assert!(c.elements < 1 << 21, "{} elements", c.elements);
    // An intersection with them, then saves, a smaller clip, a line and restores: each restore sends the clip again,
    // and the budget stops it.
    let mut e = many_rects(Emf::default(), 60_000, 1);
    for _ in 0..5_000 {
        e = e.r(33, &[]).r(30, &[0, 0, 50, 50]).r(54, &[10, 10]).r(34, &[-1]).r(54, &[20, 20]);
    }
    let c = counted(&e.bytes());
    assert!(c.elements <= super::MAX_WORK + (1 << 20), "{} elements", c.elements);
}

#[test]
fn a_recoloured_monochrome_brush_is_copied_within_the_budget() {
    // A 1024 × 1024 one-bit brush, filled with in 4000 text colours.
    let rows: Vec<Vec<u8>> = (0..1024).map(|_| vec![0xAA; 128]).collect();
    let (bmi, bits) = dib(1024, 1024, 1, &[0, 0, 0, 0, 255, 255, 255, 0], &rows.iter().map(Vec::as_slice).collect::<Vec<_>>(), 0);
    let mut b = le(&[1, 0, 32, bmi.len() as i32, 32 + bmi.len() as i32, bits.len() as i32]);
    b.extend(&bmi);
    b.extend(&bits);
    let mut e = Emf::default().rec(93, &b).r(37, &[1]);
    for i in 0..4000 {
        e = e.r(24, &[i]).r(43, &[0, 0, 10, 10]);
    }
    let c = counted(&e.bytes());
    // Each copy is 24 bits a pixel, counted so against the 2^26 pixels a metafile may draw: at most 22 copies.
    assert!(c.bitmaps.len() <= 22 && c.bitmap_bytes < 100 << 20, "{} bitmaps, {} bytes", c.bitmaps.len(), c.bitmap_bytes);
}
