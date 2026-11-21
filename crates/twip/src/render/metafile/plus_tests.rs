//! The EMF+ player: records drawn into a log and on the CPU, and hostile files.

use std::time::Instant;

use super::tests::{BLUE, Emf, GREEN, RED, WHITE, at, counted, drawn, le, log, pixels, stand_in, survives};

/// `EmfPlusGraphicsVersion`.
const V: u32 = 0xDBC0_1002;
const OPAQUE_RED: u32 = 0xFFFF_0000;
const OPAQUE_BLUE: u32 = 0xFF00_00FF;
const OPAQUE_WHITE: u32 = 0xFFFF_FFFF;

fn f32s(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn u32s(v: &[u32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

/// An EMF+ record of `kind`, `flags` and `data`, padded to four bytes (its data size the data's own).
fn rec(kind: u16, flags: u16, data: &[u8]) -> Vec<u8> {
    let mut d = data.to_vec();
    d.resize(d.len().next_multiple_of(4), 0);
    let mut r = Vec::new();
    r.extend(kind.to_le_bytes());
    r.extend(flags.to_le_bytes());
    r.extend(((d.len() + 12) as u32).to_le_bytes());
    r.extend((data.len() as u32).to_le_bytes());
    r.extend(d);
    r
}

/// The records in one EMF+ comment.
fn plus(e: Emf, records: &[Vec<u8>]) -> Emf {
    let mut c = le(&[0x2B46_4D45]);
    for r in records {
        c.extend(r);
    }
    let mut b = le(&[c.len() as i32]);
    b.extend(c);
    e.rec(70, &b)
}

/// The EMF+ header of a dual file or an EMF+-only one, at the test device's 100 dpi.
fn header(dual: bool) -> Vec<u8> {
    rec(0x4001, u16::from(dual), &u32s(&[V, 1, 100, 100]))
}

fn fill_rects(colour: u32, rects: &[[f32; 4]]) -> Vec<u8> {
    let mut d = u32s(&[colour, rects.len() as u32]);
    for r in rects {
        d.extend(f32s(r));
    }
    rec(0x400A, 0x8000, &d)
}

/// An object record: `kind` (`ObjectType`) at `id`.
fn object(kind: u16, id: u16, data: &[u8]) -> Vec<u8> {
    rec(0x4008, kind << 8 | id, data)
}

fn solid(argb: u32) -> Vec<u8> {
    u32s(&[V, 0, argb])
}

/// A pen of `width` world units with `flags` and their optional data, in `brush`.
fn pen(width: f32, flags: u32, optional: &[u8], brush: &[u8]) -> Vec<u8> {
    let mut d = u32s(&[V, 0, flags, 0]);
    d.extend(f32s(&[width]));
    d.extend(optional);
    d.extend(brush);
    d
}

/// An `EmfPlusPath` of float points and their types.
fn path(pts: &[[f32; 2]], types: &[u8]) -> Vec<u8> {
    let mut d = u32s(&[V, pts.len() as u32, 0]);
    for p in pts {
        d.extend(f32s(p));
    }
    d.extend(types);
    d
}

fn start() -> Emf {
    plus(Emf::default(), &[header(true)])
}

// ---------------- dual files ----------------

#[test]
fn a_dual_file_draws_its_emf_plus_records_not_the_plain_ones() {
    let e = plus(Emf::default(), &[header(true), fill_rects(OPAQUE_RED, &[[10.0, 10.0, 40.0, 40.0]])]).r(43, &[0, 0, 50, 50]);
    assert_eq!(drawn(&log(e)), ["fill 7.2,7.2,36.0,36.0 #ff0000"]);
    // An EMF+-only file draws the same.
    let e = plus(Emf::default(), &[header(false), fill_rects(OPAQUE_RED, &[[10.0, 10.0, 40.0, 40.0]])]);
    assert_eq!(drawn(&log(e)), ["fill 7.2,7.2,36.0,36.0 #ff0000"]);
}

#[test]
fn plain_records_after_get_dc_draw_until_the_next_emf_plus_comment() {
    let e = start().r(43, &[0, 0, 10, 10]);
    let e = plus(e, &[rec(0x4004, 0, &[])]).r(43, &[0, 0, 50, 50]);
    let e = plus(e, &[fill_rects(OPAQUE_BLUE, &[[0.0, 0.0, 20.0, 20.0]])]).r(43, &[0, 0, 90, 90]);
    assert_eq!(drawn(&log(e)), ["fill 0.0,0.0,36.0,36.0 #ffffff", "stroke 0.0,0.0,36.0,36.0 0.72 #000000", "fill 0.0,0.0,14.4,14.4 #0000ff"]);
}

#[test]
fn the_dual_fixture_draws_its_emf_plus_fill() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/render/metafile-emf.docx");
    let d = crate::docx::read(&std::fs::read(path).expect("fixture")).expect("reads");
    let dual = d.images.iter().find(|i| i.windows(4).any(|w| w == b"EMF+")).expect("the dual picture");
    let mut l = super::tests::Log::default();
    assert!(super::emf::play(&mut l, dual));
    assert_eq!(drawn(&l.ops), ["fill 7.2,7.2,208.8,64.8 #70ad47", "stroke 7.2,36.0,208.8,36.0 2.16 #385723"]);
}

// ---------------- transforms, saves and containers ----------------

#[test]
fn world_and_page_transforms_place_shapes() {
    // Half an inch a unit: 50 device pixels.
    let e = plus(start(), &[rec(0x4030, 4, &f32s(&[0.5])), fill_rects(OPAQUE_RED, &[[0.0, 0.0, 1.0, 1.0]])]);
    assert_eq!(drawn(&log(e))[0], "fill 0.0,0.0,36.0,36.0 #ff0000");
    // Moved, then doubled after.
    let e = plus(
        start(),
        &[rec(0x402D, 0, &f32s(&[10.0, 0.0])), rec(0x402E, 0x2000, &f32s(&[2.0, 2.0])), fill_rects(OPAQUE_RED, &[[0.0, 0.0, 10.0, 10.0]])],
    );
    assert_eq!(drawn(&log(e))[0], "fill 14.4,0.0,28.8,14.4 #ff0000");
    // Doubled, then moved first: the move is doubled too.
    let e =
        plus(start(), &[rec(0x402E, 0, &f32s(&[2.0, 2.0])), rec(0x402D, 0, &f32s(&[10.0, 0.0])), fill_rects(OPAQUE_RED, &[[0.0, 0.0, 10.0, 10.0]])]);
    assert_eq!(drawn(&log(e))[0], "fill 14.4,0.0,28.8,14.4 #ff0000");
    // A quarter turn about the origin.
    let e = plus(start(), &[rec(0x402F, 0, &f32s(&[90.0])), fill_rects(OPAQUE_RED, &[[0.0, 0.0, 10.0, 20.0]])]);
    assert_eq!(drawn(&log(e))[0], "fill -14.4,0.0,0.0,7.2 #ff0000");
}

#[test]
fn saves_restore_and_containers_map_their_contents() {
    let e = plus(
        start(),
        &[
            rec(0x402D, 0, &f32s(&[10.0, 0.0])),
            rec(0x4025, 0, &u32s(&[7])),
            rec(0x402D, 0, &f32s(&[50.0, 0.0])),
            rec(0x4026, 0, &u32s(&[7])),
            fill_rects(OPAQUE_RED, &[[0.0, 0.0, 10.0, 10.0]]),
        ],
    );
    assert_eq!(drawn(&log(e))[0], "fill 7.2,0.0,14.4,7.2 #ff0000");
    // A container drawing its 10 × 10 units into 20 × 20 from (10, 10); its end puts the transform back.
    let mut b = f32s(&[10.0, 10.0, 20.0, 20.0, 0.0, 0.0, 10.0, 10.0]);
    b.extend(u32s(&[3]));
    let e = plus(
        start(),
        &[
            rec(0x4027, 2, &b),
            fill_rects(OPAQUE_RED, &[[0.0, 0.0, 10.0, 10.0]]),
            rec(0x4029, 0, &u32s(&[3])),
            fill_rects(OPAQUE_BLUE, &[[0.0, 0.0, 10.0, 10.0]]),
        ],
    );
    assert_eq!(drawn(&log(e)), ["fill 7.2,7.2,21.6,21.6 #ff0000", "fill 0.0,0.0,7.2,7.2 #0000ff"]);
}

// ---------------- clips ----------------

/// The picture with the clip set by `clips` ((rect, mode) each), then everything filled red: rendered.
fn clipped(clips: &[([f32; 4], u16)]) -> Vec<[u8; 4]> {
    let mut r: Vec<Vec<u8>> = clips.iter().map(|(rect, mode)| rec(0x4032, mode << 8, &f32s(rect))).collect();
    r.push(fill_rects(OPAQUE_RED, &[[0.0, 0.0, 100.0, 100.0]]));
    pixels(plus(start(), &r))
}

fn row(px: &[[u8; 4]]) -> [[u8; 3]; 3] {
    [at(px, 20, 50), at(px, 50, 50), at(px, 80, 50)]
}

#[test]
fn clips_combine_by_every_mode() {
    let left = [0.0, 0.0, 60.0, 100.0];
    let right = [40.0, 0.0, 60.0, 100.0];
    assert_eq!(row(&clipped(&[(left, 0), (right, 1)])), [WHITE, RED, WHITE], "intersect");
    assert_eq!(row(&clipped(&[([0.0, 0.0, 30.0, 100.0], 0), ([70.0, 0.0, 30.0, 100.0], 2)])), [RED, WHITE, RED], "union");
    assert_eq!(row(&clipped(&[(left, 0), (right, 3)])), [RED, WHITE, RED], "xor");
    assert_eq!(row(&clipped(&[(left, 0), (right, 4)])), [RED, WHITE, WHITE], "exclude");
    assert_eq!(row(&clipped(&[(left, 0), (right, 5)])), [WHITE, WHITE, RED], "complement");
    // Reset, the whole picture draws.
    let px = pixels(plus(start(), &[rec(0x4032, 0, &f32s(&left)), rec(0x4031, 0, &[]), fill_rects(OPAQUE_RED, &[[0.0, 0.0, 100.0, 100.0]])]));
    assert_eq!(row(&px), [RED; 3]);
}

/// A region node: a rect.
fn rect_node(r: [f32; 4]) -> Vec<u8> {
    let mut d = u32s(&[0x1000_0000]);
    d.extend(f32s(&r));
    d
}

#[test]
fn region_trees_clip_and_fill_exactly() {
    // (left ∪ right) less the middle third of the left, as a clip.
    let mut region = u32s(&[V, 4, 4, 2]);
    region.extend(rect_node([0.0, 0.0, 30.0, 100.0]));
    region.extend(rect_node([70.0, 0.0, 30.0, 100.0]));
    region.extend(rect_node([10.0, 0.0, 15.0, 100.0]));
    let px = pixels(plus(start(), &[object(4, 1, &region), rec(0x4034, 1, &[]), fill_rects(OPAQUE_RED, &[[0.0, 0.0, 100.0, 100.0]])]));
    assert_eq!([at(&px, 5, 50), at(&px, 15, 50), at(&px, 27, 50), at(&px, 50, 50), at(&px, 80, 50)], [RED, WHITE, RED, WHITE, RED]);
    // Filled with a brush.
    let px = pixels(plus(start(), &[object(4, 1, &region), rec(0x4013, 0x8001, &u32s(&[OPAQUE_BLUE]))]));
    assert_eq!([at(&px, 5, 50), at(&px, 15, 50), at(&px, 80, 50)], [BLUE, WHITE, BLUE]);
}

// ---------------- brushes ----------------

fn filled_with(brush: &[u8]) -> Vec<[u8; 4]> {
    pixels(plus(start(), &[object(1, 2, brush), rec(0x400A, 0, &[u32s(&[2, 1]), f32s(&[0.0, 0.0, 100.0, 100.0])].concat())]))
}

fn near(a: [u8; 3], b: [u8; 3], by: u8) -> bool {
    a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= by)
}

#[test]
fn linear_gradients_run_across_their_rect() {
    let mut b = u32s(&[V, 4, 0, 0]);
    b.extend(f32s(&[0.0, 0.0, 100.0, 100.0]));
    b.extend(u32s(&[OPAQUE_RED, OPAQUE_BLUE, 0, 0]));
    let px = filled_with(&b);
    assert!(near(at(&px, 1, 50), RED, 8) && near(at(&px, 98, 50), BLUE, 8) && near(at(&px, 50, 10), [128, 0, 128], 8), "{:?}", row(&px));
}

/// A path gradient over the 100 × 100 square from white at its centre to red at its edge, with `flags` and their
/// optional data.
fn square_gradient(flags: u32, surround: &[u32], optional: &[u8]) -> Vec<u8> {
    let mut b = u32s(&[V, 3, flags, 4, OPAQUE_WHITE]);
    b.extend(f32s(&[50.0, 50.0]));
    b.extend(u32s(&[surround.len() as u32]));
    b.extend(u32s(surround));
    b.extend(u32s(&[4]));
    b.extend(f32s(&[0.0, 0.0, 100.0, 0.0, 100.0, 100.0, 0.0, 100.0]));
    b.extend(optional);
    b
}

#[test]
fn path_gradients_shade_from_their_centre_out() {
    let px = filled_with(&square_gradient(0, &[OPAQUE_RED], &[]));
    assert!(near(at(&px, 50, 50), WHITE, 6) && near(at(&px, 1, 50), RED, 8) && near(at(&px, 25, 50), [255, 128, 128], 6), "{:?}", row(&px));
    // Focus scales: the middle half square is the centre's colour.
    let mut focus = u32s(&[2]);
    focus.extend(f32s(&[0.5, 0.5]));
    let px = filled_with(&square_gradient(0x40, &[OPAQUE_RED], &focus));
    assert!(near(at(&px, 30, 50), WHITE, 6) && near(at(&px, 12, 50), [255, 128, 128], 8), "{:?}", [at(&px, 30, 50), at(&px, 12, 50)]);
    // Blend factors: the centre's colour reached half way in.
    let mut blend = u32s(&[3]);
    blend.extend(f32s(&[0.0, 0.5, 1.0, 0.0, 1.0, 1.0]));
    let px = filled_with(&square_gradient(0x08, &[OPAQUE_RED], &blend));
    assert!(near(at(&px, 26, 50), WHITE, 8) && near(at(&px, 12, 50), [255, 128, 128], 8), "{:?}", [at(&px, 26, 50), at(&px, 12, 50)]);
    // A colour for each corner, and a transparent edge over the white under it.
    let px = filled_with(&square_gradient(0, &[OPAQUE_RED, 0xFF00_FF00, OPAQUE_BLUE, 0xFF00_0000], &[]));
    assert!(near(at(&px, 1, 1), RED, 12) && near(at(&px, 98, 1), GREEN, 12) && near(at(&px, 98, 98), BLUE, 12), "corners");
    let px = filled_with(&square_gradient(0, &[0x00FF_0000], &[]));
    assert!(near(at(&px, 1, 50), WHITE, 8) && near(at(&px, 50, 50), WHITE, 8));
}

#[test]
fn path_gradient_cells_shade_within_eight_bits_of_gdi_plus() {
    // Corner colours and an uneven blend with unequal focus scales: no cell strays half an 8-bit step from the
    // gradient's colour at its middle (where the triangles bend most).
    let mut optional = u32s(&[3]);
    optional.extend(f32s(&[0.0, 0.3, 1.0, 0.0, 0.8, 1.0]));
    optional.extend(u32s(&[2]));
    optional.extend(f32s(&[0.2, 0.6]));
    let b = square_gradient(0x48, &[OPAQUE_RED, 0xFF00_FF00, OPAQUE_BLUE, 0xFF00_0000], &optional);
    let px = filled_with(&b);
    // Along the middle row, left of the centre, the colour changes smoothly: no step bigger than the gradient's.
    let steps: Vec<u32> =
        (2..48).map(|x| at(&px, x, 50).iter().zip(at(&px, x + 1, 50)).map(|(a, b)| u32::from(a.abs_diff(b))).max().unwrap()).collect();
    assert!(steps.iter().all(|&s| s <= 24), "{steps:?}");
}

#[test]
fn hatches_tile_device_pixels() {
    // Horizontal: a line every eighth row, from the top.
    let px = filled_with(&u32s(&[V, 1, 0, 0xFF00_0000, OPAQUE_WHITE]));
    assert_eq!([at(&px, 5, 0), at(&px, 5, 1), at(&px, 5, 8), at(&px, 5, 12)], [[0; 3], WHITE, [0; 3], WHITE]);
}

// ---------------- images ----------------

/// A 2 × 2 32-bit ARGB bitmap: red, green over blue, white.
fn bitmap() -> Vec<u8> {
    let mut d = u32s(&[V, 1, 2, 2, 8, 0x0026_200A, 0]);
    d.extend([0, 0, 255, 255, 0, 255, 0, 255, 255, 0, 0, 255, 255, 255, 255, 255]);
    d
}

fn draw_image(id: u16, src: [f32; 4], dst: [f32; 4]) -> Vec<u8> {
    let mut d = u32s(&[0xFFFF_FFFF, 2]);
    d.extend(f32s(&src));
    d.extend(f32s(&dst));
    rec(0x401A, id, &d)
}

#[test]
fn images_draw_their_pixels_onto_rects_and_parallelograms() {
    let px = pixels(plus(start(), &[object(5, 3, &bitmap()), draw_image(3, [0.0, 0.0, 2.0, 2.0], [0.0, 0.0, 100.0, 100.0])]));
    let got = [at(&px, 25, 25), at(&px, 75, 25), at(&px, 25, 75), at(&px, 75, 75)];
    assert!(got.iter().zip([RED, GREEN, BLUE, WHITE]).all(|(a, b)| near(*a, b, 6)), "{got:?}");
    // Its top half onto the parallelogram from (0, 0), (100, 0) and (0, 50).
    let mut d = u32s(&[0xFFFF_FFFF, 2]);
    d.extend(f32s(&[0.0, 0.0, 2.0, 1.0]));
    d.extend(u32s(&[3]));
    d.extend(f32s(&[0.0, 0.0, 100.0, 0.0, 0.0, 50.0]));
    let px = pixels(plus(start(), &[object(5, 3, &bitmap()), rec(0x401B, 3, &d)]));
    let got = [at(&px, 25, 25), at(&px, 75, 25), at(&px, 50, 75)];
    assert!(got.iter().zip([RED, GREEN, WHITE]).all(|(a, b)| near(*a, b, 6)), "{got:?}");
}

/// An `EmfPlusImage` holding an EMF.
fn metafile_image(emf: &[u8]) -> Vec<u8> {
    let mut d = u32s(&[V, 2, 3, emf.len() as u32]);
    d.extend(emf);
    d
}

#[test]
fn a_metafile_image_plays_inside_the_picture() {
    let inner = Emf::default().r(39, &[1, 0, 0xFF, 0]).r(37, &[1]).r(37, &[0x8000_0008u32 as i32]).r(43, &[0, 0, 101, 101]).bytes();
    let px = pixels(plus(start(), &[object(5, 4, &metafile_image(&inner)), draw_image(4, [0.0, 0.0, 100.0, 100.0], [50.0, 50.0, 50.0, 50.0])]));
    assert_eq!([at(&px, 25, 25), at(&px, 75, 75), at(&px, 75, 25)], [WHITE, RED, WHITE]);
}

// ---------------- pens and shapes ----------------

#[test]
fn pens_stroke_shapes_lines_and_curves() {
    let objects = [object(2, 1, &pen(2.0, 0, &[], &solid(OPAQUE_RED)))];
    let rect = f32s(&[0.0, 0.0, 100.0, 100.0]);
    let arc = [f32s(&[0.0, 90.0]), rect.clone()].concat();
    let curve = [f32s(&[0.5]), u32s(&[0, 2, 3]), f32s(&[0.0, 50.0, 50.0, 0.0, 100.0, 50.0])].concat();
    let records = [
        rec(0x400F, 1, &rect),
        rec(0x4012, 1, &arc),
        rec(0x4011, 1, &arc),
        rec(0x4018, 1, &curve),
        rec(0x400D, 0x2001, &[u32s(&[3]), f32s(&[0.0, 0.0, 50.0, 0.0, 0.0, 50.0])].concat()),
    ];
    let ops = log(plus(start(), &[objects.to_vec(), records.to_vec()].concat()));
    assert_eq!(
        drawn(&ops),
        [
            "stroke 0.0,0.0,72.0,72.0 1.44 #ff0000",
            "stroke 36.0,36.0,72.0,72.0 1.44 #ff0000",
            "stroke 36.0,36.0,72.0,72.0 1.44 #ff0000",
            "stroke 0.0,0.0,72.0,36.0 1.44 #ff0000",
            "stroke 0.0,0.0,36.0,36.0 1.44 #ff0000"
        ]
    );
}

#[test]
fn pen_styles_caps_and_alignment() {
    // Dashed: cut into dashes, the first (from the line's start) stroked apart from the rest.
    let dashed = pen(1.0, 0x20, &u32s(&[1]), &solid(OPAQUE_RED));
    let line = rec(0x400D, 1, &[u32s(&[2]), f32s(&[0.0, 50.0, 100.0, 50.0])].concat());
    assert_eq!(
        drawn(&log(plus(start(), &[object(2, 1, &dashed), line.clone()]))),
        ["stroke 2.9,36.0,71.3,36.0 0.72 #ff0000", "stroke 0.0,36.0,2.2,36.0 0.72 #ff0000"]
    );
    // A filled arrow at the end (a custom cap) draws beside the line.
    let mut cap = u32s(&[V, 1]);
    cap.extend(f32s(&[4.0, 4.0, 0.0]));
    cap.extend(u32s(&[1, 0, 0, 0]));
    cap.extend(f32s(&[10.0, 1.0, 0.0, 0.0, 0.0, 0.0]));
    let mut optional = u32s(&[0xFF]);
    optional.extend(u32s(&[cap.len() as u32]));
    optional.extend(&cap);
    let arrow = pen(2.0, 0x4 | 0x1000, &optional, &solid(OPAQUE_RED));
    let ops = log(plus(start(), &[object(2, 1, &arrow), line.clone()]));
    let d = drawn(&ops);
    assert!(d.len() == 2 && d[0].starts_with("stroke 0.0,") && d[1].starts_with("fill") && d[1].contains(",72.0,"), "{d:?}");
    // A linear gradient pen strokes with it; a path gradient pen fills its outline with the gradient's mesh.
    let mut g = u32s(&[V, 4, 0, 0]);
    g.extend(f32s(&[0.0, 0.0, 100.0, 100.0]));
    g.extend(u32s(&[OPAQUE_RED, OPAQUE_BLUE, 0, 0]));
    let ops = log(plus(start(), &[object(2, 1, &pen(4.0, 0, &[], &g)), line.clone()]));
    assert_eq!(drawn(&ops), ["stroke 0.0,36.0,72.0,36.0 2.88 gradient"]);
    let px = pixels(plus(start(), &[object(2, 1, &pen(10.0, 0, &[], &square_gradient(0, &[OPAQUE_BLUE], &[]))), line]));
    assert!(near(at(&px, 2, 50), BLUE, 16) && near(at(&px, 50, 50), WHITE, 8) && at(&px, 50, 40) == WHITE, "{:?}", [at(&px, 2, 50), at(&px, 50, 50)]);
    // An inset pen around a rect draws inside it alone.
    let inset = pen(20.0, 0x200, &u32s(&[1]), &solid(OPAQUE_RED));
    let px = pixels(plus(start(), &[object(2, 1, &inset), rec(0x400B, 1, &[u32s(&[1]), f32s(&[20.0, 20.0, 60.0, 60.0])].concat())]));
    assert_eq!([at(&px, 15, 50), at(&px, 25, 50), at(&px, 50, 50)], [WHITE, RED, WHITE]);
}

#[test]
fn a_compound_pen_draws_its_strips() {
    // Two strips: the outer fifths either side.
    let mut optional = u32s(&[4]);
    optional.extend(f32s(&[0.0, 0.2, 0.8, 1.0]));
    let compound = pen(50.0, 0x400, &optional, &solid(OPAQUE_RED));
    let line = rec(0x400D, 1, &[u32s(&[2]), f32s(&[0.0, 50.0, 100.0, 50.0])].concat());
    let px = pixels(plus(start(), &[object(2, 1, &compound), line]));
    assert_eq!([at(&px, 50, 28), at(&px, 50, 40), at(&px, 50, 50), at(&px, 50, 72)], [RED, WHITE, WHITE, RED]);
}

#[test]
fn paths_fill_and_stroke_as_objects() {
    let p = path(&[[10.0, 10.0], [90.0, 10.0], [90.0, 90.0], [10.0, 90.0]], &[0, 1, 1, 0x81]);
    let ops = log(plus(
        start(),
        &[
            object(3, 5, &p),
            object(2, 1, &pen(1.0, 0, &[], &solid(OPAQUE_BLUE))),
            rec(0x4014, 0x8005, &u32s(&[OPAQUE_RED])),
            rec(0x4015, 5, &u32s(&[1])),
        ],
    ));
    assert_eq!(drawn(&ops), ["fill 7.2,7.2,64.8,64.8 #ff0000", "stroke 7.2,7.2,64.8,64.8 0.72 #0000ff"]);
}

#[test]
fn relative_points_and_run_length_types() {
    // Points from (10, 10), then +80 across, +80 down and -80 back (each past seven bits, in two bytes); the types
    // run-length encoded, the last closing.
    let mut d = u32s(&[V, 4, 0x800]);
    d.extend([10, 10, 0x80, 0x50, 0, 0, 0x80, 0x50, 0xFF, 0xB0, 0]);
    d.extend([1, 0, 2, 1, 1, 0x81]);
    let ops = log(plus(start(), &[object(3, 5, &d), rec(0x4014, 0x8005, &u32s(&[OPAQUE_RED]))]));
    assert_eq!(drawn(&ops), ["fill 7.2,7.2,64.8,64.8 #ff0000"]);
}

// ---------------- text ----------------

fn font(em: f32) -> Vec<u8> {
    let name: Vec<u8> = "Arial".encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut d = u32s(&[V]);
    d.extend(f32s(&[em]));
    d.extend(u32s(&[2, 0, 0, 5]));
    d.extend(name);
    d
}

#[test]
fn strings_break_into_lines_in_their_rect() {
    let s: Vec<u8> = "ab cd ef".encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut d = u32s(&[OPAQUE_RED, 0xFFFF_FFFF, 8]);
    d.extend(f32s(&[0.0, 0.0, 50.0, 100.0]));
    d.extend(&s);
    let ops = log(plus(start(), &[object(6, 1, &font(20.0)), rec(0x401C, 0x8001, &d)]));
    let texts: Vec<&str> = drawn(&ops).into_iter().filter(|o| o.starts_with("text")).collect();
    assert_eq!(texts.len(), 3, "{texts:?}");
    assert!(texts[0].starts_with("text \"ab \" Arial at 2.4,0.0") && texts[2].starts_with("text \"ef\" Arial at 2.4,28.8"), "{texts:?}");
}

#[test]
fn driver_strings_place_each_glyph() {
    let mut d = u32s(&[OPAQUE_RED, 1, 0, 2]);
    d.extend([b'A', 0, b'B', 0]);
    d.extend(f32s(&[10.0, 50.0, 40.0, 50.0]));
    let ops = log(plus(start(), &[object(6, 1, &font(20.0)), rec(0x4036, 0x8001, &d)]));
    assert_eq!(drawn(&ops), ["text \"AB\" Arial at 7.2,36.0 [30.0, 10.0] rises [0.0, 0.0]"]);
}

#[test]
fn strings_draw_with_fonts() {
    let s: Vec<u8> = "Hello".encode_utf16().flat_map(u16::to_le_bytes).collect();
    let mut d = u32s(&[0xFF00_0000, 0xFFFF_FFFF, 5]);
    d.extend(f32s(&[0.0, 0.0, 100.0, 100.0]));
    d.extend(&s);
    let e = plus(start(), &[object(6, 1, &font(30.0)), rec(0x401C, 0x8001, &d)]);
    let px = crate::render::graphics::Graphics::metafile_pixels(&e.bytes(), 100, 100, Some(&stand_in())).expect("draws");
    assert!(px.iter().filter(|p| p[0] < 100).count() > 50, "ink");
}

// ---------------- hostile files ----------------

#[test]
fn counts_of_two_to_the_thirty_two_read_nothing() {
    let max = u32::MAX;
    let records = [
        rec(0x400A, 0x8000, &u32s(&[OPAQUE_RED, max])),
        rec(0x400C, 0x8000, &u32s(&[OPAQUE_RED, max])),
        rec(0x400D, 1, &u32s(&[max])),
        rec(0x401C, 0x8001, &u32s(&[OPAQUE_RED, 0, max, 0, 0, 0, 0])),
        rec(0x4036, 0x8001, &u32s(&[OPAQUE_RED, 1, 0, max])),
        object(3, 2, &u32s(&[V, max, 0])),
        object(3, 2, &u32s(&[V, max, 0x800])),
        object(4, 2, &u32s(&[V, max, 2, 2, 2, 2])),
        object(5, 2, &u32s(&[V, 1, 0x7FFF_FFFF, 0x7FFF_FFFF, 0x7FFF_FFFF, 0x0026_200A, 0])),
        object(5, 2, &u32s(&[V, 1, 2, 2, 4, 0x0003_0803, 0, 0, max])),
        object(1, 2, &u32s(&[V, 3, 0, 0, 0, 0, 0, max])),
        object(2, 2, &u32s(&[V, 0, 0x100, 0, 0, max])),
        object(6, 2, &u32s(&[V, 0, 0, 0, 0, max])),
    ];
    assert!(drawn(&survives(&plus(start(), &records).bytes())).is_empty());
}

#[test]
fn objects_in_parts_keep_only_their_bytes() {
    // An object claiming 4 GiB in parts of a few bytes, then one whose parts complete it.
    let mut e = start();
    for _ in 0..1000 {
        e = plus(e, &[rec(0x4008, 0x8100 | 3, &u32s(&[u32::MAX, 0, 0]))]);
    }
    let brush = solid(OPAQUE_RED);
    let (a, b) = brush.split_at(5);
    let e = plus(
        e,
        &[
            rec(0x4008, 0x8100 | 4, &[u32s(&[brush.len() as u32]), a.to_vec()].concat()),
            rec(0x4008, 0x8100 | 4, &[u32s(&[brush.len() as u32]), b.to_vec()].concat()),
            rec(0x400A, 4, &[u32s(&[4, 1]), f32s(&[0.0, 0.0, 10.0, 10.0])].concat()),
        ],
    );
    assert_eq!(drawn(&survives(&e.bytes())), ["fill 0.0,0.0,7.2,7.2 #ff0000"]);
}

#[test]
fn a_deep_region_reads_bounded() {
    // 100 000 nested unions: past the node bound, the region is nothing.
    let mut region = u32s(&[V, 200_001]);
    for _ in 0..100_000 {
        region.extend(u32s(&[2]));
    }
    for _ in 0..100_001 {
        region.extend(rect_node([0.0, 0.0, 1.0, 1.0]));
    }
    let ops = survives(&plus(start(), &[object(4, 1, &region), rec(0x4034, 1, &[]), rec(0x4013, 0x8001, &u32s(&[OPAQUE_RED]))]).bytes());
    assert!(drawn(&ops).is_empty());
    // The deepest a region within the bound nests still plays, quickly.
    let mut region = u32s(&[V, 16_000]);
    for _ in 0..8_000 {
        region.extend(u32s(&[3]));
    }
    for _ in 0..8_001 {
        region.extend(rect_node([0.0, 0.0, 1.0, 1.0]));
    }
    counted(&plus(start(), &[object(4, 1, &region), rec(0x4034, 2 << 8 | 1, &[]), fill_rects(OPAQUE_RED, &[[0.0, 0.0, 10.0, 10.0]])]).bytes());
}

#[test]
fn clips_replayed_by_saves_are_counted() {
    // A 20 000-point clip path, then saves, smaller clips (intersecting and unioned), draws and restores: each
    // restore sends the big clip again, or a new mask with it, and the budget stops them.
    let pts: Vec<[f32; 2]> = (0..20_000).map(|i| [(i % 100) as f32, (i / 200) as f32]).collect();
    let mut types = vec![1u8; pts.len()];
    types[0] = 0;
    for mode in [1u16, 2] {
        let mut r = vec![object(3, 1, &path(&pts, &types)), rec(0x4033, 1, &[])];
        for i in 0..5_000u32 {
            r.push(rec(0x4025, 0, &u32s(&[i])));
            r.push(rec(0x4032, mode << 8, &f32s(&[0.0, 0.0, 50.0, 50.0])));
            r.push(fill_rects(OPAQUE_RED, &[[0.0, 0.0, 10.0, 10.0]]));
            r.push(rec(0x4026, 0, &u32s(&[i])));
            r.push(fill_rects(OPAQUE_RED, &[[0.0, 0.0, 10.0, 10.0]]));
        }
        let c = counted(&plus(start(), &r).bytes());
        assert!(c.elements <= super::MAX_WORK + (1 << 20), "mode {mode}: {} elements", c.elements);
    }
}

#[test]
fn bitmaps_made_again_and_again_stay_within_the_budget() {
    // A 1024 × 1024 bitmap object made 200 times, each drawn; then a flipping texture of it made in a fresh slot
    // each time.
    let mut big = u32s(&[V, 1, 1024, 1024, 4096, 0x0026_200A, 0]);
    big.extend(vec![0x80; 1024 * 4096]);
    let mut r = Vec::new();
    for _ in 0..200 {
        r.push(object(5, 1, &big));
        r.push(draw_image(1, [0.0, 0.0, 1024.0, 1024.0], [0.0, 0.0, 10.0, 10.0]));
    }
    let c = counted(&plus(start(), &r).bytes());
    assert!(c.bitmap_bytes <= 300 << 20, "{} bytes in {} bitmaps", c.bitmap_bytes, c.bitmaps.len());
    let mut texture = u32s(&[V, 2, 0, 3]);
    texture.extend(&big);
    let mut r = Vec::new();
    for i in 0..100 {
        r.push(object(1, i % 60, &texture));
        r.push(rec(0x400A, i % 60, &[u32s(&[u32::from(i % 60), 1]), f32s(&[0.0, 0.0, 100.0, 100.0])].concat()));
    }
    let c = counted(&plus(start(), &r).bytes());
    assert!(c.bitmap_bytes <= 300 << 20, "{} bytes in {} bitmaps", c.bitmap_bytes, c.bitmaps.len());
}

#[test]
fn hatches_in_many_colours_stay_within_the_budget() {
    let mut r = Vec::new();
    for i in 0..200_000u32 {
        r.push(object(1, 1, &u32s(&[V, 1, 5, 0xFF00_0000 | i, OPAQUE_WHITE])));
        r.push(rec(0x400A, 1, &[u32s(&[1, 1]), f32s(&[0.0, 0.0, 10.0, 10.0])].concat()));
    }
    let c = counted(&plus(start(), &r).bytes());
    assert!(c.bitmap_bytes <= 64 << 20, "{} bytes in {} bitmaps", c.bitmap_bytes, c.bitmaps.len());
}

/// A metafile in which each level draws the one inside it `k` times, `levels` deep, over an innermost picture of
/// `records` records that draw nothing.
fn nested(k: usize, levels: usize, records: usize) -> Vec<u8> {
    let mut e = Emf::default();
    for _ in 0..records {
        e = e.r(18, &[1]);
    }
    let mut inner = e.bytes();
    for _ in 0..levels {
        let mut r = vec![header(false), object(5, 1, &metafile_image(&inner))];
        for _ in 0..k {
            r.push(draw_image(1, [0.0, 0.0, 100.0, 100.0], [0.0, 0.0, 100.0, 100.0]));
        }
        inner = plus(Emf::default(), &r).bytes();
    }
    inner
}

/// The picture drawn at 100 × 100 pixels on the CPU, in at most `secs` seconds optimised (the renderer unoptimised,
/// as tests build it, takes about twelve times as long).
fn drawn_within(d: &[u8], secs: u64) {
    drawn_at_within(d, (100, 100), secs);
}

fn drawn_at_within(d: &[u8], (w, h): (u32, u32), secs: u64) {
    let start = Instant::now();
    crate::render::graphics::Graphics::metafile_pixels(d, w, h, None).expect("draws");
    let limit = std::time::Duration::from_secs(if cfg!(debug_assertions) { secs * 12 } else { secs });
    assert!(start.elapsed() < limit, "drawn in {:?}", start.elapsed());
}

#[test]
fn metafiles_nest_a_bounded_depth_on_one_budget() {
    // Each level draws the one inside it a hundred times, ten levels deep, over a picture that draws nothing: each
    // record read, and each picture placed, is work all the same.
    let d = nested(100, 10, 50);
    let start = Instant::now();
    let c = counted(&d);
    assert!(c.calls <= super::MAX_WORK, "{} calls in {:?}", c.calls, start.elapsed());
    // Four levels of 60 over an empty picture (a 7 KB file that took 6.7 s and 5.6 GB to draw).
    drawn_within(&nested(60, 4, 0), 1);
}

/// A small tiled path gradient (a triangle, its brush scaled by `scale`) filling the picture `fills` times.
fn tiled_mesh(fills: usize, scale: f32) -> Vec<u8> {
    let mut b = u32s(&[V, 3, 0x02, 0, OPAQUE_WHITE]);
    b.extend(f32s(&[50.0, 50.0]));
    b.extend(u32s(&[1, OPAQUE_RED, 3]));
    b.extend(f32s(&[0.0, 0.0, 100.0, 0.0, 50.0, 100.0]));
    b.extend(f32s(&[scale, 0.0, 0.0, scale, 0.0, 0.0]));
    let mut r = vec![header(false), object(1, 2, &b)];
    for _ in 0..fills {
        r.push(rec(0x400A, 2, &[u32s(&[2, 1]), f32s(&[0.0, 0.0, 100.0, 100.0])].concat()));
    }
    plus(Emf::default(), &r).bytes()
}

#[test]
fn meshes_are_priced_by_what_they_draw() {
    // 10 000 fills of a tiled path gradient (a 7.9 KB file that took 30 s), and 300 of one in tiles a tenth the size.
    drawn_within(&tiled_mesh(10_000, 1.0), 2);
    drawn_within(&tiled_mesh(300, 0.1), 2);
}

#[test]
fn shown_hotkeys_lay_out_in_linear_time() {
    let s = "&a".repeat(20_000) + &"\n".repeat(20_000);
    let chars: Vec<u8> = s.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let format = [u32s(&[V, 0, 0, 0, 0, 0, 0]), f32s(&[0.0]), u32s(&[1]), f32s(&[0.0, 0.0, 1.0]), u32s(&[0, 0, 0])].concat();
    let mut d = u32s(&[OPAQUE_RED, 2, s.encode_utf16().count() as u32]);
    d.extend(f32s(&[0.0, 0.0, 0.0, 0.0]));
    d.extend(&chars);
    let e = plus(start(), &[object(6, 1, &font(20.0)), object(7, 2, &format), rec(0x401C, 0x8001, &d)]);
    let start = Instant::now();
    survives(&e.bytes());
    assert!(start.elapsed() < std::time::Duration::from_secs(2), "{:?}", start.elapsed());
}

#[test]
fn a_huge_gradient_boundary_is_bounded_before_its_triangles() {
    // A million-point boundary (8 MB): its triangles are counted as they are made.
    let n = 1_000_000u32;
    let mut b = u32s(&[V, 3, 0, 4, OPAQUE_WHITE]);
    b.extend(f32s(&[50.0, 50.0]));
    b.extend(u32s(&[1, OPAQUE_RED, n]));
    for i in 0..n {
        let a = f64::from(i) / f64::from(n) * std::f64::consts::TAU;
        b.extend(f32s(&[(50.0 + 50.0 * a.cos()) as f32, (50.0 + 50.0 * a.sin()) as f32]));
    }
    let e = plus(start(), &[object(1, 2, &b), rec(0x400A, 2, &[u32s(&[2, 1]), f32s(&[0.0, 0.0, 100.0, 100.0])].concat())]);
    let start = Instant::now();
    counted(&e.bytes());
    assert!(start.elapsed() < std::time::Duration::from_secs(10), "{:?}", start.elapsed());
}

#[test]
fn a_path_gradient_with_many_points_is_bounded() {
    // A 100 000-point boundary, every point its own colour, with uneven focus scales and a blend.
    let n = 100_000u32;
    let mut b = u32s(&[V, 3, 0x48, 4, OPAQUE_WHITE]);
    b.extend(f32s(&[50.0, 50.0]));
    b.extend(u32s(&[n]));
    b.extend((0..n).flat_map(|i| (0xFF00_0000 | (i * 97)).to_le_bytes()));
    b.extend(u32s(&[n]));
    for i in 0..n {
        let a = f64::from(i) / f64::from(n) * std::f64::consts::TAU;
        b.extend(f32s(&[(50.0 + 50.0 * a.cos()) as f32, (50.0 + 50.0 * a.sin()) as f32]));
    }
    b.extend(u32s(&[3]));
    b.extend(f32s(&[0.0, 0.3, 1.0, 0.0, 0.8, 1.0]));
    b.extend(u32s(&[2]));
    b.extend(f32s(&[0.2, 0.6]));
    let mut r = vec![object(1, 2, &b)];
    for _ in 0..20 {
        r.push(rec(0x400A, 2, &[u32s(&[2, 1]), f32s(&[0.0, 0.0, 100.0, 100.0])].concat()));
    }
    let start = Instant::now();
    let c = counted(&plus(Emf::default(), &[vec![header(false)], r].concat()).bytes());
    assert!(c.elements <= super::MAX_WORK + (1 << 20), "{} elements in {:?}", c.elements, start.elapsed());
}

#[test]
fn truncated_and_oversized_records_stop_the_comment() {
    let good = fill_rects(OPAQUE_RED, &[[0.0, 0.0, 10.0, 10.0]]);
    for size in [0u32, 7, 13, 1 << 31] {
        let mut bad = good.clone();
        bad[4..8].copy_from_slice(&size.to_le_bytes());
        let ops = survives(&plus(start(), &[bad, good.clone()]).bytes());
        assert!(drawn(&ops).is_empty(), "size {size}: {ops:?}");
    }
    // A data size past the record reads within it.
    let mut long = good.clone();
    long[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    assert_eq!(drawn(&survives(&plus(start(), &[long]).bytes())), ["fill 0.0,0.0,7.2,7.2 #ff0000"]);
}

#[test]
fn clear_and_copy_compositing_replace_what_is_under() {
    // Red, then cleared to half-transparent blue: over the white page, a light blue, not purple.
    let px = pixels(plus(start(), &[fill_rects(OPAQUE_RED, &[[0.0, 0.0, 100.0, 100.0]]), rec(0x4009, 0, &u32s(&[0x800000FF]))]));
    assert!(near(at(&px, 50, 50), [127, 127, 255], 3), "{:?}", at(&px, 50, 50));
    // A translucent fill in copy mode replaces the red under it too.
    let px = pixels(plus(
        start(),
        &[fill_rects(OPAQUE_RED, &[[0.0, 0.0, 100.0, 100.0]]), rec(0x4023, 1, &[]), fill_rects(0x800000FF, &[[0.0, 0.0, 50.0, 100.0]])],
    ));
    assert!(near(at(&px, 25, 50), [127, 127, 255], 3) && at(&px, 75, 50) == RED, "{:?}", row(&px));
}

#[test]
fn image_attributes_tile_and_clamp() {
    // The bitmap's source rect twice its size: tiled, it repeats; clamped to a colour, the rest is that colour.
    let attributes = |wrap: u32, clamp: u32| object(8, 6, &u32s(&[V, 0, wrap, clamp, 0, 0]));
    let draw = |c: &[u8]| {
        let mut d = u32s(&[6, 2]);
        d.extend(f32s(&[0.0, 0.0, 4.0, 4.0]));
        d.extend(f32s(&[0.0, 0.0, 100.0, 100.0]));
        rec(0x401A, 3, &[c, &d[4..]].concat())
    };
    let id = u32s(&[6]);
    let px = pixels(plus(start(), &[object(5, 3, &bitmap()), attributes(0, 0), rec(0x4021, 5, &[]), draw(&id)]));
    assert_eq!([at(&px, 12, 12), at(&px, 62, 12), at(&px, 87, 87)], [RED, RED, WHITE]);
    let px = pixels(plus(start(), &[object(5, 3, &bitmap()), attributes(4, OPAQUE_BLUE), rec(0x4021, 5, &[]), draw(&id)]));
    assert_eq!([at(&px, 12, 12), at(&px, 62, 12), at(&px, 87, 87)], [RED, BLUE, BLUE]);
}

#[test]
fn a_metafile_texture_tiles_on_the_budget() {
    // A metafile texture (a 100 × 100 pixel EMF of a red rect) tiled at a hundredth of its size over the picture.
    let inner = Emf::default().r(39, &[1, 0, 0xFF, 0]).r(37, &[1]).r(43, &[0, 0, 50, 50]).bytes();
    let mut texture = u32s(&[V, 2, 0x02, 0]);
    texture.extend(f32s(&[0.01, 0.0, 0.0, 0.01, 0.0, 0.0]));
    texture.extend(metafile_image(&inner));
    let mut r = vec![object(1, 2, &texture)];
    for _ in 0..50 {
        r.push(rec(0x400A, 2, &[u32s(&[2, 1]), f32s(&[0.0, 0.0, 100.0, 100.0])].concat()));
    }
    let c = counted(&plus(start(), &r).bytes());
    assert!(c.elements <= super::MAX_WORK + (1 << 20), "{} elements", c.elements);
    // At its own size, it draws.
    let mut texture = u32s(&[V, 2, 0, 0]);
    texture.extend(metafile_image(&inner));
    let px = pixels(plus(start(), &[object(1, 2, &texture), rec(0x400A, 2, &[u32s(&[2, 1]), f32s(&[0.0, 0.0, 100.0, 100.0])].concat())]));
    assert_eq!([at(&px, 20, 20), at(&px, 75, 25)], [RED, WHITE]);
}

#[test]
fn dashed_lines_cap_their_ends_and_their_dashes_apart() {
    // Dashes 3 widths long with triangle caps; the line's start round. A 10-wide pen along y = 50 from x = 20.
    let dashed = pen(10.0, 0x2 | 0x20 | 0x40, &u32s(&[2, 1, 3]), &solid(OPAQUE_RED));
    let line = rec(0x400D, 1, &[u32s(&[2]), f32s(&[20.0, 50.0, 95.0, 50.0])].concat());
    let px = pixels(plus(start(), &[object(2, 1, &dashed), line]));
    // The round start reaches 5 back at the middle, not at the corners; a dash's triangle reaches past its end at
    // the middle alone.
    assert_eq!([at(&px, 16, 50), at(&px, 16, 45)], [RED, WHITE], "the round start");
    assert_eq!([at(&px, 52, 50), at(&px, 52, 46)], [RED, WHITE], "a triangle after the first dash");
}

#[test]
fn miters_past_their_limit_are_clipped_or_bevelled() {
    // A sharp turn at (50, 20): the miter reaches far past a limit of 2. Clipped (a miter join), the corner reaches
    // the limit, 2 half widths up from the point; bevelled (a clipped miter join), it stops near the point.
    let path = [u32s(&[3]), f32s(&[30.0, 80.0, 50.0, 20.0, 70.0, 80.0])].concat();
    let draw = |join: u32| {
        let p = pen(10.0, 0x8 | 0x10, &[u32s(&[join]), f32s(&[2.0])].concat(), &solid(OPAQUE_RED));
        pixels(plus(start(), &[object(2, 1, &p), rec(0x400D, 1, &path)]))
    };
    let (clipped, bevelled) = (draw(0), draw(3));
    assert_eq!(at(&clipped, 50, 12), RED, "clipped at the limit");
    assert_eq!(at(&clipped, 50, 9), WHITE, "and no further");
    assert_eq!(at(&bevelled, 50, 12), WHITE, "bevelled");
}

#[test]
fn a_lopsided_compound_pen_draws_each_strip_on_its_side() {
    // Strips at 0 to 0.6 and 0.8 to 1 of a 50-wide pen running right along y = 50: the thick strip on its left
    // (above), the thin one below.
    let mut optional = u32s(&[4]);
    optional.extend(f32s(&[0.0, 0.6, 0.8, 1.0]));
    let compound = pen(50.0, 0x400, &optional, &solid(OPAQUE_RED));
    let line = rec(0x400D, 1, &[u32s(&[2]), f32s(&[0.0, 50.0, 100.0, 50.0])].concat());
    let px = pixels(plus(start(), &[object(2, 1, &compound), line]));
    assert_eq!([at(&px, 50, 28), at(&px, 50, 52), at(&px, 50, 60), at(&px, 50, 72), at(&px, 50, 78)], [RED, RED, WHITE, RED, WHITE]);
}

#[test]
fn a_shown_hotkey_prefix_underlines_its_character() {
    let s: Vec<u8> = "&Save && close".encode_utf16().flat_map(u16::to_le_bytes).collect();
    let format = [u32s(&[V, 0, 0, 0, 0, 0, 0]), f32s(&[0.0]), u32s(&[1]), f32s(&[0.0, 0.0, 1.0]), u32s(&[0, 0, 0])].concat();
    let mut d = u32s(&[OPAQUE_RED, 2, 14]);
    d.extend(f32s(&[0.0, 0.0, 0.0, 0.0]));
    d.extend(&s);
    let ops = log(plus(start(), &[object(6, 1, &font(20.0)), object(7, 2, &format), rec(0x401C, 0x8001, &d)]));
    let texts: Vec<&str> = drawn(&ops).into_iter().filter(|o| o.starts_with("text")).collect();
    assert!(texts.len() == 2 && texts[0].starts_with("text \"S\"") && texts[1].starts_with("text \"ave & close\" Arial at 7.2,"), "{texts:?}");
}

#[test]
fn a_copy_under_a_mask_clip_replaces_inside_it_alone() {
    // Red, then a half-transparent blue copied under the union of two rects.
    let px = pixels(plus(
        start(),
        &[
            fill_rects(OPAQUE_RED, &[[0.0, 0.0, 100.0, 100.0]]),
            rec(0x4032, 0, &f32s(&[0.0, 0.0, 30.0, 100.0])),
            rec(0x4032, 2 << 8, &f32s(&[70.0, 0.0, 30.0, 100.0])),
            rec(0x4023, 1, &[]),
            fill_rects(0x800000FF, &[[0.0, 0.0, 100.0, 100.0]]),
        ],
    ));
    assert!(near(at(&px, 15, 50), [127, 127, 255], 3) && at(&px, 50, 50) == RED && near(at(&px, 85, 50), [127, 127, 255], 3), "{:?}", row(&px));
}

#[test]
fn fills_are_aliased_unless_anti_aliasing_is_on() {
    // A rect from 10.5: its edge column half covered, drawn whole or not at all when aliased, half when not.
    let r = fill_rects(0xFF00_0000, &[[10.5, 10.0, 50.0, 50.0]]);
    let aliased = pixels(plus(start(), std::slice::from_ref(&r)));
    let smooth = pixels(plus(start(), &[rec(0x401E, 0x09, &[]), r]));
    assert!(matches!(at(&aliased, 10, 30), [0, 0, 0] | [255, 255, 255]), "{:?}", at(&aliased, 10, 30));
    assert!(at(&smooth, 10, 30)[0] > 60 && at(&smooth, 10, 30)[0] < 200, "{:?}", at(&smooth, 10, 30));
}

#[test]
fn dashes_corners_and_strips_stay_within_the_budget() {
    // A 100 000-point zigzag stroked, again and again, by a pen with tiny triangle-capped dashes, clipped miters
    // and lopsided compound strips.
    let pts: Vec<f32> = (0..100_000).flat_map(|i| [(i % 100) as f32, (i % 2 * 50) as f32]).collect();
    let line = rec(0x400D, 1, &[u32s(&[100_000]), f32s(&pts)].concat());
    let optional = [u32s(&[0]), f32s(&[2.0]), u32s(&[1, 3]), u32s(&[4]), f32s(&[0.0, 0.6, 0.8, 1.0])].concat();
    let p = pen(0.1, 0x8 | 0x10 | 0x20 | 0x40 | 0x400, &optional, &solid(0x80FF_0000));
    let mut r = vec![object(2, 1, &p)];
    for _ in 0..20 {
        r.push(line.clone());
    }
    let start = Instant::now();
    let c = counted(&plus(super::tests::Emf::default(), &[vec![header(false)], r].concat()).bytes());
    assert!(c.elements <= super::MAX_WORK + (1 << 21), "{} elements in {:?}", c.elements, start.elapsed());
}

#[test]
fn meshes_cost_their_share_of_the_picture_whatever_its_frame() {
    // 20 000 fills of a tiled path gradient in a picture whose frame is a quarter of a millimetre across, drawn at
    // 3 × 1.5 inches (a 9 KB file that took 11.7 s), and larger still.
    let mut d = tiled_mesh(20_000, 1.0);
    d[32..40].copy_from_slice(&super::tests::le(&[25, 25]));
    assert!(super::emf::Header::read(&d).is_some_and(|h| h.size.width < 1.0), "a tiny frame");
    drawn_at_within(&d, (288, 144), 2);
    drawn_at_within(&d, (1152, 576), 8);
}

#[test]
fn saves_and_containers_nest_bounded_both_ways() {
    // 20 000 containers and saves begun and never ended, each with a clip; then 20 000 ends of states never begun,
    // under the full stack; a fill after each.
    let mut r = Vec::new();
    for i in 0..20_000u32 {
        let begin = if i % 2 == 0 {
            rec(0x4027, 2, &[f32s(&[0.0, 0.0, 99.0, 99.0, 0.0, 0.0, 100.0, 100.0]), u32s(&[i])].concat())
        } else {
            rec(0x4025, 0, &u32s(&[i]))
        };
        r.push(begin);
        r.push(rec(0x4032, 1 << 8, &f32s(&[0.0, 0.0, 90.0, 90.0])));
        r.push(fill_rects(OPAQUE_RED, &[[0.0, 0.0, 10.0, 10.0]]));
    }
    for i in 0..20_000u32 {
        r.push(rec(if i % 2 == 0 { 0x4029 } else { 0x4026 }, 0, &u32s(&[u32::MAX - i])));
        r.push(fill_rects(OPAQUE_RED, &[[0.0, 0.0, 10.0, 10.0]]));
    }
    let d = plus(start(), &r).bytes();
    let start = Instant::now();
    let c = counted(&d);
    assert!(c.calls <= super::MAX_WORK && start.elapsed() < std::time::Duration::from_secs(5), "{} calls in {:?}", c.calls, start.elapsed());
    drawn_within(&d, 2);
}

// ---------------- hostile pens ----------------

/// A pen one pixel wide in a solid colour, with `flags` and their optional data.
fn pixel_pen(flags: u32, optional: &[u8]) -> Vec<u8> {
    let mut d = u32s(&[V, 0, flags, 2]);
    d.extend(f32s(&[1.0]));
    d.extend(optional);
    d.extend(solid(0xFF00_0000));
    d
}

#[test]
fn dashes_of_a_thousandth_of_a_width_on_a_long_line_stroke_solid() {
    // A custom dash of 1e-6 on and off (held to a thousandth of the width) along a line 1e5 long: 5e7 dashes.
    let optional = [u32s(&[5, 2]), f32s(&[1e-6, 1e-6])].concat();
    let pen = pixel_pen(0x20 | 0x100, &optional);
    let mut line = u32s(&[2]);
    line.extend(f32s(&[0.0, 0.0, 1e5, 0.0]));
    let d = plus(super::tests::Emf::default(), &[header(false), object(2, 1, &pen), rec(0x400D, 1, &line)]).bytes();
    let start = Instant::now();
    let c = counted(&d);
    assert!(c.elements <= 64 && start.elapsed() < std::time::Duration::from_secs(2), "{} elements in {:?}", c.elements, start.elapsed());
}

#[test]
fn a_custom_cap_on_a_huge_curve_pulls_back_without_looping() {
    // One cubic (0,0), (1e11,1e11), (-1e11,1e11), (0,1) under a custom arrow end cap.
    let mut cap = u32s(&[0, 1]);
    cap.extend(f32s(&[1.0, 1.0, 0.0]));
    cap.extend(u32s(&[1, 0, 0, 0]));
    cap.extend(f32s(&[0.0, 1.0]));
    let optional = [u32s(&[0xFF, 1, cap.len() as u32]), cap].concat();
    let pen = pixel_pen(0x4 | 0x8 | 0x1000, &optional);
    for s in [1e11f32, 1e18, 1e30, f32::MAX] {
        let mut path = u32s(&[V, 4, 0]);
        path.extend(f32s(&[0.0, 0.0, s, s, -s, s, 0.0, 1.0]));
        path.extend([0u8, 3, 3, 3]);
        let d = plus(super::tests::Emf::default(), &[header(false), object(2, 1, &pen), object(3, 2, &path), rec(0x4015, 2, &u32s(&[1]))]).bytes();
        counted(&d);
    }
}

#[test]
fn miter_corners_between_huge_curves_are_bounded() {
    // `k` looping Béziers of 1e11 across under the default miter join: each join measures two huge segments.
    let pen = pixel_pen(0, &[]);
    for (k, s) in [(1_000usize, 1e11f32), (20_000, 1e11), (20_000, 1e30)] {
        let mut pts = vec![0.0f32, 0.0];
        for _ in 0..k {
            pts.extend([s, s, -s, s, 0.0, 0.0]);
        }
        let mut path = u32s(&[(1 + 3 * k) as u32]);
        path.extend(f32s(&pts));
        let d = plus(super::tests::Emf::default(), &[header(false), object(2, 1, &pen), rec(0x4019, 1, &path)]).bytes();
        let start = Instant::now();
        let c = counted(&d);
        assert!(
            c.elements <= super::MAX_WORK + (1 << 21) && start.elapsed() < std::time::Duration::from_secs(5),
            "{k}: {} elements in {:?}",
            c.elements,
            start.elapsed()
        );
    }
}
