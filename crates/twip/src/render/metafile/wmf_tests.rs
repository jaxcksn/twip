//! The WMF player: records drawn into a log and on the CPU, and hostile files.

use super::tests::{Count, Emf, Log, dib, drawn, le, many_rects, within};
use super::wmf::{Header, play};

/// A placeable WMF made record by record: its window 100 units across on a 72 dpi inch, so a unit is a point.
#[derive(Default)]
struct Wmf {
    body: Vec<u8>,
    records: u32,
}

impl Wmf {
    /// A record of `kind` holding `params`, padded to a word.
    fn rec(mut self, kind: u16, params: &[u8]) -> Wmf {
        let mut p = params.to_vec();
        p.resize(p.len().next_multiple_of(2), 0);
        self.body.extend(((p.len() as u32 + 6) / 2).to_le_bytes());
        self.body.extend(kind.to_le_bytes());
        self.body.extend(p);
        self.records += 1;
        self
    }

    /// A record whose parameters are 16-bit words.
    fn w(self, kind: u16, v: &[i16]) -> Wmf {
        self.rec(kind, &v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>())
    }

    /// `META_CREATEBRUSHINDIRECT`: a solid brush.
    fn brush(self, colour: u32) -> Wmf {
        self.rec(0x02FC, &[&[0, 0][..], &colour.to_le_bytes(), &[0, 0]].concat())
    }

    /// `META_CREATEPENINDIRECT`.
    fn pen(self, style: u16, width: i16, colour: u32) -> Wmf {
        self.rec(0x02FA, &[&style.to_le_bytes()[..], &width.to_le_bytes(), &[0, 0], &colour.to_le_bytes()].concat())
    }

    fn colour_text(self, c: u32) -> Wmf {
        self.rec(0x0209, &c.to_le_bytes())
    }

    fn select(self, i: i16) -> Wmf {
        self.w(0x012D, &[i])
    }

    /// `META_RECTANGLE` of (left, top, right, bottom).
    fn rect(self, l: i16, t: i16, r: i16, b: i16) -> Wmf {
        self.w(0x041B, &[b, r, t, l])
    }

    fn bytes(self) -> Vec<u8> {
        let mut d = 0x9AC6_CDD7u32.to_le_bytes().to_vec();
        for v in [0i16, 0, 0, 100, 100] {
            d.extend(v.to_le_bytes());
        }
        d.extend(72u16.to_le_bytes());
        d.extend([0; 6]);
        let eof = [3u32.to_le_bytes().as_slice(), &[0, 0]].concat();
        let words = (18 + self.body.len() + eof.len()) / 2;
        d.extend([1u16, 9, 0x300].iter().flat_map(|v| v.to_le_bytes()));
        d.extend((words as u32).to_le_bytes());
        d.extend(8u16.to_le_bytes());
        d.extend((self.body.len() as u32 / 2).to_le_bytes());
        d.extend(0u16.to_le_bytes());
        d.extend(self.body);
        d.extend(eof);
        d
    }
}

fn log(w: Wmf) -> Vec<String> {
    let mut l = Log::default();
    assert!(play(&mut l, &w.bytes()), "plays");
    assert_eq!(l.depth, 0, "saves and restores balance: {:?}", l.ops);
    l.ops
}

fn pixels(w: Wmf) -> Vec<[u8; 4]> {
    crate::render::graphics::Graphics::metafile_pixels(&w.bytes(), 100, 100, None).expect("draws")
}

#[test]
fn the_header_gives_the_extent_in_points() {
    let d = Wmf::default().bytes();
    let h = Header::read(&d).expect("a header");
    assert_eq!((h.size.width.round(), h.size.height.round()), (100.0, 100.0));
    assert_eq!(crate::docx::image::image_type(&d), Some(("image/x-wmf", "wmf")));
    let (w, h, dx, dy) = crate::docx::image::image_info(&d).expect("a size");
    assert_eq!((w.round(), h.round(), dx, dy), (133.0, 133.0, 96.0, 96.0));
}

#[test]
fn a_plain_file_is_sized_by_its_window() {
    // No placeable header: SetWindowOrg, SetWindowExt of 200 × 100 units, a unit a pixel (0.75 pt).
    let d = Wmf::default().w(0x020B, &[10, 20]).w(0x020C, &[100, 200]).bytes()[22..].to_vec();
    let h = Header::read(&d).expect("a header");
    assert_eq!((h.size.width.round(), h.size.height.round()), (150.0, 75.0));
    assert_eq!(crate::docx::image::image_type(&d), Some(("image/x-wmf", "wmf")));
    // Without a window it has no extent to draw in.
    assert!(Header::read(&Wmf::default().rect(0, 0, 5, 5).bytes()[22..]).is_none());
}

#[test]
fn a_rectangle_fills_and_strokes_in_points() {
    let ops = log(Wmf::default().brush(0x0000FF).select(0).rect(10, 10, 60, 60));
    assert_eq!(drawn(&ops), ["fill 10.0,10.0,60.0,60.0 #ff0000", "stroke 10.0,10.0,60.0,60.0 0.75 #000000"]);
}

#[test]
fn objects_take_the_lowest_free_index() {
    // Brushes 0 and 1; 0 deleted, so the next brush takes 0 again and the pen 2.
    let w = Wmf::default().brush(0xFF).brush(0xFF00).w(0x01F0, &[0]).brush(0xFF0000).pen(5, 0, 0).select(1).select(2).rect(0, 0, 50, 50);
    assert_eq!(drawn(&log(w)), ["fill 0.0,0.0,50.0,50.0 #00ff00"]);
    // The same, selecting the reused index.
    let w = Wmf::default().brush(0xFF).brush(0xFF00).w(0x01F0, &[0]).brush(0xFF0000).pen(5, 0, 0).select(0).select(2).rect(0, 0, 50, 50);
    assert_eq!(drawn(&log(w)), ["fill 0.0,0.0,50.0,50.0 #0000ff"]);
}

#[test]
fn the_window_maps_onto_the_viewport() {
    // Anisotropic, 1000 units across the picture's 100 points, from (500, 500).
    let w = Wmf::default().w(0x0103, &[8]).w(0x020C, &[1000, 1000]).w(0x020B, &[500, 500]).pen(5, 0, 0).select(0).rect(500, 500, 1000, 1000);
    assert_eq!(drawn(&log(w))[0], "fill 0.0,0.0,50.0,50.0 #ffffff");
    // A viewport of 144 × 108 pixels from (144, 0): a 100-unit window fills it.
    let w =
        Wmf::default().w(0x0103, &[8]).w(0x020C, &[100, 100]).w(0x020E, &[108, 144]).w(0x020D, &[0, 144]).pen(5, 0, 0).select(0).rect(0, 0, 100, 100);
    assert_eq!(drawn(&log(w))[0], "fill 108.0,0.0,216.0,81.0 #ffffff");
    // Hundredths of a millimetre, y up: 25.4 mm is 96 pixels, 72 points.
    let w = Wmf::default().w(0x0103, &[3]).pen(5, 0, 0).select(0).rect(0, 0, 1270, -1270);
    assert_eq!(drawn(&log(w))[0], "fill 0.0,0.0,36.0,36.0 #ffffff");
}

#[test]
fn polygons_lines_and_text_draw() {
    let w = Wmf::default().pen(5, 0, 0).select(0).w(0x0324, &[3, 10, 10, 50, 10, 30, 50]);
    assert_eq!(drawn(&log(w)), ["fill 10.0,10.0,50.0,50.0 #ffffff"]);
    // Two polygons in one fill.
    let w = Wmf::default().pen(5, 0, 0).select(0).w(0x0538, &[2, 3, 3, 0, 0, 10, 0, 5, 5, 20, 20, 30, 20, 25, 30]);
    assert_eq!(drawn(&log(w)), ["fill 0.0,0.0,30.0,30.0 #ffffff"]);
    // TextOut of "Hi" at (5, 20).
    let w = Wmf::default().w(0x0521, &[2, i16::from_le_bytes(*b"Hi"), 20, 5]);
    let ops = log(w);
    assert!(drawn(&ops)[0].starts_with("text \"Hi\" Arial at 5.0,20.0"), "{ops:?}");
}

#[test]
fn bitmaps_draw_their_pixels() {
    // A 2 × 1 24-bit DIB, red then blue, stretched over the left half through StretchDIBits.
    let (bmi, bits) = dib(2, 1, 24, &[], &[&[0, 0, 255, 255, 0, 0]], 0);
    let mut p = 0x00CC_0020u32.to_le_bytes().to_vec();
    p.extend([0u8, 0]);
    p.extend([1i16, 2, 0, 0, 100, 50, 0, 0].iter().flat_map(|v: &i16| v.to_le_bytes()));
    p.extend(&bmi);
    p.extend(&bits);
    let px = pixels(Wmf::default().rec(0x0F43, &p));
    assert_eq!((px[50 * 100 + 5], px[50 * 100 + 45], px[50 * 100 + 75]), ([255, 0, 0, 255], [0, 0, 255, 255], [255; 4]));
    // A device bitmap (monochrome: a one bit is the white background, a zero the black text colour), stretched over the picture.
    let mut p = 0x00CC_0020u32.to_le_bytes().to_vec();
    p.extend([1i16, 8, 0, 0, 100, 100, 0, 0].iter().flat_map(|v: &i16| v.to_le_bytes()));
    p.extend([0i16, 8, 1, 2].iter().flat_map(|v: &i16| v.to_le_bytes()));
    p.extend([1u8, 1, 0b1010_1010, 0]);
    let px = pixels(Wmf::default().rec(0x0B23, &p));
    assert_eq!((px[50 * 100 + 5], px[50 * 100 + 15]), ([255; 4], [0, 0, 0, 255]));
}

#[test]
fn regions_clip_and_fill() {
    // A region of two scans (one run each): rows 0..10 from 0..20, rows 10..20 from 0..10.
    let scans = [0u16, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0, 0, 2, 0, 10, 0, 0, 20, 2, 0];
    let mut p: Vec<u8> = scans.iter().flat_map(|v| v.to_le_bytes()).collect();
    // The object header: next in chain, type 6, count, size, scan count 2, max scan, bounds.
    p[2..4].copy_from_slice(&6u16.to_le_bytes());
    p[10..12].copy_from_slice(&2u16.to_le_bytes());
    let mut r = vec![0u8; 22];
    r[2..4].copy_from_slice(&6u16.to_le_bytes());
    r[10..12].copy_from_slice(&2u16.to_le_bytes());
    for s in [(2u16, 0u16, 10u16, 0u16, 20u16), (2, 10, 20, 0, 10)] {
        r.extend([s.0, s.1, s.2, s.3, s.4, 2].iter().flat_map(|v| v.to_le_bytes()));
    }
    let w = Wmf::default().rec(0x06FF, &r).brush(0xFF).select(1).w(0x012B, &[0]);
    assert_eq!(drawn(&log(w)), ["fill 0.0,0.0,20.0,20.0 #ff0000"]);
}

// ---------------- hostile files ----------------

/// Plays `d`, which must finish quickly and leave the sink balanced.
fn survives(d: &[u8]) -> Vec<String> {
    let mut l = Log::default();
    within(5, || play(&mut l, d));
    assert_eq!(l.depth, 0);
    l.ops
}

fn counted(d: &[u8]) -> Count {
    let mut c = Count::default();
    within(60, || play(&mut c, d));
    assert_eq!(c.depth, 0);
    c
}

#[test]
fn a_truncated_header_isnt_a_wmf() {
    let d = Wmf::default().rect(0, 0, 50, 50).bytes();
    for n in [0, 4, 10, 21, 30, 39] {
        assert!(Header::read(&d[..n]).is_none(), "{n} bytes");
        assert!(!play(&mut Log::default(), &d[..n]));
    }
    // A header of the wrong size or version.
    for (at, v) in [(24usize, 8u8), (26, 7)] {
        let mut d = d.clone();
        d[at] = v;
        assert!(Header::read(&d).is_none());
    }
}

#[test]
fn a_short_record_stops_the_file() {
    let mut d = Wmf::default().rect(0, 0, 50, 50).bytes();
    // The rectangle's size in words, set to nothing, to two, and to more than the file holds.
    for words in [0u32, 2, 1 << 31, u32::MAX] {
        d[40..44].copy_from_slice(&words.to_le_bytes());
        assert!(drawn(&survives(&d)).is_empty(), "{words} words");
    }
}

#[test]
fn counts_of_sixty_five_thousand_read_nothing() {
    let max = -1;
    for w in [
        Wmf::default().w(0x0325, &[max, 0, 0]),
        Wmf::default().w(0x0538, &[max, max, 1, 0, 0]),
        Wmf::default().w(0x0521, &[max, 0]),
        Wmf::default().w(0x0A32, &[0, 0, max, 0, 0]),
        Wmf::default().w(0x00F7, &[0x300, max, 0]),
        Wmf::default().w(0x06FF, &[0, 6, 0, 0, 0, 0, max]),
        Wmf::default().w(0x0F43, &[0, 0, 0, 0, 2, 1, 0, 0, 100, 50, 0, 0, 0]),
    ] {
        assert!(drawn(&survives(&w.bytes())).is_empty());
    }
}

#[test]
fn a_bitmap_too_big_isnt_drawn() {
    // A 65536 × 65536 header with 8 bytes of bits; and a device bitmap 32767 square with none.
    let mut p = 0x00CC_0020u32.to_le_bytes().to_vec();
    p.extend([0u8, 0]);
    p.extend([1i16, 2, 0, 0, 100, 50, 0, 0].iter().flat_map(|v: &i16| v.to_le_bytes()));
    p.extend(le(&[40, 65536, 65536, 0x0018_0001, 0, 8, 0, 0, 0, 0]));
    p.extend([0; 8]);
    assert!(drawn(&survives(&Wmf::default().rec(0x0F43, &p).bytes())).is_empty());
    let mut p = 0x00CC_0020u32.to_le_bytes().to_vec();
    p.extend([1i16, 1, 0, 0, 100, 100, 0, 0, 0, 32767, 32767, 4096].iter().flat_map(|v: &i16| v.to_le_bytes()));
    p.extend([1u8, 1]);
    assert!(drawn(&survives(&Wmf::default().rec(0x0B23, &p).bytes())).is_empty());
}

#[test]
fn objects_naming_nothing_or_everything() {
    // Selected and deleted past the table, a delete twice, then enough creates to fill it.
    let mut w =
        Wmf::default().select(0).select(-1).select(30_000).w(0x01F0, &[5]).w(0x01F0, &[-1]).brush(0xFF).select(0).w(0x01F0, &[0]).w(0x01F0, &[0]);
    for _ in 0..70_000 {
        w = w.brush(0xFF);
    }
    let ops = survives(&w.select(0).rect(0, 0, 50, 50).bytes());
    assert!(drawn(&ops).len() <= 2);
}

#[test]
fn a_save_storm_keeps_a_bounded_stack() {
    let mut w = Wmf::default();
    for _ in 0..200_000 {
        w = w.w(0x001E, &[]);
    }
    let mut w = w.w(0x0127, &[i16::MIN]).w(0x0127, &[1]).w(0x0127, &[i16::MAX]);
    for _ in 0..10_000 {
        w = w.w(0x0416, &[50, 50, 0, 0]);
    }
    let ops = survives(&w.rect(0, 0, 50, 50).bytes());
    assert!(ops.iter().filter(|o| o.starts_with("clip")).count() <= super::MAX_CLIPS, "the clip stays bounded");
}

#[test]
fn degenerate_windows_and_huge_coordinates() {
    let mut w = Wmf::default().w(0x0103, &[8]).w(0x020C, &[0, 0]).w(0x020E, &[i16::MAX, i16::MIN]);
    w = w.pen(2, 0, 0).select(0).w(0x0325, &[2, i16::MIN, i16::MIN, i16::MAX, i16::MAX]).w(0x0418, &[i16::MAX, i16::MAX, i16::MIN, i16::MIN]);
    let d = w.bytes();
    survives(&d);
    within(5, || crate::render::graphics::Graphics::metafile_pixels(&d, 50, 50, None));
    // Scaling the window up, and down, over and over leaves it where it last made sense: what is drawn has a place.
    for (num, den) in [(i16::MAX, 1), (1, i16::MAX)] {
        let mut w = Wmf::default().w(0x0103, &[8]);
        for _ in 0..200 {
            w = w.w(0x0410, &[den, num, den, num]).w(0x0412, &[den, num, den, num]);
        }
        let ops = log(w.pen(5, 0, 0).select(0).rect(0, 0, 50, 50));
        assert!(drawn(&ops).iter().all(|o| !o.contains("NaN") && !o.contains("inf")), "{ops:?}");
    }
}

/// `META_CREATEREGION` of `n` one-pixel scans down a column, and a `META_SELECTCLIPREGION` of it.
fn many_scans(w: Wmf, n: u16) -> Wmf {
    let mut r = vec![0u8; 22];
    r[2..4].copy_from_slice(&6u16.to_le_bytes());
    r[10..12].copy_from_slice(&n.to_le_bytes());
    for i in 0..n {
        r.extend([2u16, i, i + 1, 0, 1, 2].iter().flat_map(|v| v.to_le_bytes()));
    }
    w.rec(0x06FF, &r)
}

#[test]
fn a_big_clip_is_sent_once_and_counted() {
    // A region of 60 000 rects selected as the clip, then 20 000 lines under it: it is sent once.
    let mut w = many_scans(Wmf::default(), 60_000).w(0x012C, &[0]);
    for i in 0..20_000 {
        w = w.w(0x0213, &[i % 100, 50]);
    }
    let c = counted(&w.bytes());
    assert!(c.elements < 1 << 21, "{} elements", c.elements);
    // The same clip, then saves, a smaller clip, a line and restores: each restore sends the clip again, and the
    // budget stops it.
    let mut w = many_scans(Wmf::default(), 60_000).w(0x012C, &[0]);
    for _ in 0..5_000 {
        w = w.w(0x001E, &[]).w(0x0416, &[50, 50, 0, 0]).w(0x0213, &[10, 10]).w(0x0127, &[-1]).w(0x0213, &[20, 20]);
    }
    let c = counted(&w.bytes());
    assert!(c.elements <= super::MAX_WORK + (1 << 20), "{} elements", c.elements);
}

#[test]
fn a_recoloured_monochrome_brush_is_copied_within_the_budget() {
    // A 1024 × 1024 one-bit pattern brush (CREATEPATTERNBRUSH), filled with in 4000 text colours.
    let mut p = [0i16, 1024, 1024, 128].iter().flat_map(|v: &i16| v.to_le_bytes()).collect::<Vec<u8>>();
    p.extend([1u8, 1, 0, 0, 0, 0]);
    p.extend([0u8; 18]);
    p.extend(vec![0xAA; 128 * 1024]);
    let mut w = Wmf::default().rec(0x01F9, &p).select(0);
    for i in 0..4000u32 {
        w = w.rec(0x0209, &i.to_le_bytes()).rect(0, 0, 10, 10);
    }
    let c = counted(&w.bytes());
    // Each copy is 24 bits a pixel, counted so against the 2^26 pixels a metafile may draw: at most 22 copies.
    assert!(c.bitmaps.len() <= 22 && c.bitmap_bytes < 100 << 20, "{} bitmaps, {} bytes", c.bitmaps.len(), c.bitmap_bytes);
}

/// `META_ESCAPE` chunks carrying `emf`, each `n` bytes at most, as `ENHANCED_METAFILE` comments.
fn escapes(w: Wmf, emf: &[u8], n: usize) -> Wmf {
    escapes_declaring(w, emf, n, emf.len())
}

/// [`escapes`], each chunk declaring `total` bytes for the whole.
fn escapes_declaring(mut w: Wmf, emf: &[u8], n: usize, total: usize) -> Wmf {
    let chunks: Vec<&[u8]> = emf.chunks(n).collect();
    let mut remaining = emf.len();
    for c in &chunks {
        remaining -= c.len();
        let mut p = 0x7FFFu16.to_le_bytes().to_vec();
        p.extend(0u16.to_le_bytes());
        p.extend(0x4346_4D57u32.to_le_bytes());
        p.extend([1u32, 0x10000].iter().flat_map(|v| v.to_le_bytes()));
        p.extend([0u8; 2]);
        p.extend([0u32, chunks.len() as u32, c.len() as u32, remaining as u32, total as u32].iter().flat_map(|v| v.to_le_bytes()));
        p.extend(*c);
        w = w.rec(0x0626, &p);
    }
    w
}

#[test]
fn an_embedded_emf_is_the_picture() {
    // A red square over the EMF's whole 72 pt frame, in two chunks, scaled onto the file's 100 pt; what follows is
    // not drawn.
    let emf = Emf::default().r(39, &[1, 0, 0xFF, 0]).r(37, &[1]).r(37, &[0x8000_0008u32.cast_signed()]).r(43, &[0, 0, 100, 100]).bytes();
    // Records before it draw nothing beside the EMF either.
    let before = || Wmf::default().brush(0xFF00).select(0).rect(0, 0, 10, 10);
    let w = escapes(before(), &emf, emf.len() / 2 + 1).brush(0xFF00).select(1).rect(0, 0, 10, 10);
    let ops = log(escapes(before(), &emf, emf.len() / 2 + 1).brush(0xFF00).select(1).rect(0, 0, 10, 10));
    assert_eq!(drawn(&ops), ["transform", "fill 0.0,0.0,72.0,72.0 #ff0000"]);
    let px = pixels(w);
    assert_eq!((px[95 * 100 + 95], px[5 * 100 + 5]), ([255, 0, 0, 255], [255, 0, 0, 255]));
}

#[test]
fn an_embedded_emf_is_the_picture_only_at_the_size_its_chunks_declare() {
    let emf = Emf::default().r(39, &[1, 0, 0xFF, 0]).r(37, &[1]).r(43, &[0, 0, 100, 100]).bytes();
    let red = |total: usize| {
        let w = escapes_declaring(Wmf::default(), &emf, emf.len() / 2 + 1, total).brush(0xFF00).select(0).rect(0, 0, 10, 10);
        drawn(&log(w)).iter().any(|o| o.ends_with("#ff0000"))
    };
    assert!(red(emf.len()), "the whole EMF");
    assert!(!red(emf.len() + 8), "a total the chunks don't reach: the file's own records draw");
}

#[test]
fn embedded_emf_chunks_are_bounded() {
    // Chunks claiming 4 GB, and a size past what is there.
    let mut p = 0x7FFFu16.to_le_bytes().to_vec();
    p.extend(0u16.to_le_bytes());
    p.extend(0x4346_4D57u32.to_le_bytes());
    p.extend([1u32, 0x10000].iter().flat_map(|v| v.to_le_bytes()));
    p.extend([0u8; 2]);
    p.extend([0u32, 1, u32::MAX, u32::MAX, u32::MAX].iter().flat_map(|v| v.to_le_bytes()));
    let w = Wmf::default().rec(0x0626, &p).rec(0x0626, &p).rect(0, 0, 50, 50);
    assert_eq!(drawn(&survives(&w.bytes())).len(), 2, "the file still draws");
    // A big clip in an embedded EMF: sent once, and counted, as when the EMF stands alone.
    let mut e = many_rects(Emf::default().r(30, &[0, 0, 100, 100]), 60_000, 2);
    for i in 0..20_000 {
        e = e.r(54, &[i % 100, 50]);
    }
    let c = counted(&escapes(Wmf::default(), &e.bytes(), 1 << 16).bytes());
    assert!(c.elements <= super::MAX_WORK + (1 << 20), "{} elements", c.elements);
    // Not an EMF, and a chunk sequence that never ends.
    survives(&escapes(Wmf::default(), &[7; 100], 10).bytes());
}

#[test]
fn pattern_brushes_and_device_bitmaps_of_four_and_eight_bits() {
    // CREATEPATTERNBRUSH of a 1-bit 8 × 8 bitmap: ink and background by the text and background colours.
    let mut p = [0i16, 8, 8, 2].iter().flat_map(|v: &i16| v.to_le_bytes()).collect::<Vec<u8>>();
    p.extend([1u8, 1, 0, 0, 0, 0]);
    p.extend([0u8; 18]);
    p.extend([0xF0u8, 0].repeat(8));
    let w = Wmf::default().colour_text(0x0000FF).rec(0x01F9, &p).select(0).pen(5, 0, 0).select(1).rect(0, 0, 100, 100);
    let px = pixels(w);
    assert!(px.iter().any(|p| p[..3] == [255, 0, 0]) && px.iter().any(|p| p[..3] == [255, 255, 255]), "ink and ground");
    // An 8-bit device bitmap: its pixels index the selected palette.
    let mut b = 0x00CC_0020u32.to_le_bytes().to_vec();
    b.extend([1i16, 2, 0, 0, 100, 100, 0, 0].iter().flat_map(|v: &i16| v.to_le_bytes()));
    b.extend([0i16, 2, 1, 2].iter().flat_map(|v: &i16| v.to_le_bytes()));
    b.extend([1u8, 8, 0, 1]);
    let pal = Wmf::default();
    let entries = [0xC0u8, 0, 0, 0, 0, 0xC0, 0, 0];
    let mut q = [0x300i16, 2].iter().flat_map(|v: &i16| v.to_le_bytes()).collect::<Vec<u8>>();
    q.extend(entries);
    let w = pal.rec(0x00F7, &q).w(0x0234, &[0]).rec(0x0B23, &b);
    let px = pixels(w);
    assert_eq!((px[50 * 100 + 10], px[50 * 100 + 90]), ([0xC0, 0, 0, 255], [0, 0xC0, 0, 255]));
}

#[test]
fn text_spacing_and_charsets() {
    let hi = i16::from_le_bytes(*b"A ");
    // Extra after each character and each space, restored with the saved state.
    let w = Wmf::default().w(0x0108, &[10]).w(0x020A, &[1, 6]).w(0x0521, &[2, hi, 20, 5]).w(0x001E, &[]).w(0x0108, &[0]).w(0x0127, &[-1]);
    let ops = log(w.w(0x0521, &[2, hi, 40, 5]));
    assert!(drawn(&ops).iter().all(|o| o.contains("extra 10.0 space 6.0")), "{ops:?}");
    // A symbol font's characters are in the private page.
    let mut font = [-12i16, 0, 0, 0, 400].iter().flat_map(|v: &i16| v.to_le_bytes()).collect::<Vec<u8>>();
    font.extend([0u8, 0, 0, 2, 0, 0, 0, 0]);
    font.extend(b"Symbol".iter().chain(&[0; 26]));
    let w = Wmf::default().rec(0x02FB, &font).select(0).w(0x0521, &[1, i16::from_le_bytes(*b"a\0"), 20, 5]);
    let ops = log(w);
    assert!(drawn(&ops)[0].contains("u{f061}"), "{ops:?}");
}

#[test]
fn code_pages_name_their_characters() {
    use super::emf::narrow;
    // One character from the upper half of each page, by GDI charset.
    for (charset, byte, want) in [
        (238u8, 0xA3u8, 0x141u32),
        (204, 0xC0, 0x410),
        (0, 0x80, 0x20AC),
        (161, 0xE1, 0x3B1),
        (162, 0xD0, 0x11E),
        (177, 0xE0, 0x5D0),
        (178, 0xC7, 0x627),
        (186, 0xE0, 0x105),
        (163, 0xC3, 0x102),
        (222, 0xA1, 0xE01),
        (2, 0x41, 0xF041),
    ] {
        let mut out = Vec::new();
        assert!(narrow(&mut out, &[b'a', byte], charset));
        assert_eq!(out, [if charset == 2 { 0xF061 } else { u32::from(b'a') }, want], "charset {charset}");
    }
    // The double-byte sets draw nothing.
    for charset in [128u8, 129, 130, 134, 136] {
        let mut out = Vec::new();
        assert!(!narrow(&mut out, b"ab", charset) && out.is_empty());
    }
}

#[test]
fn text_decodes_by_the_fonts_charset_in_both_formats() {
    let font = |charset: u8| {
        let mut f = [-12i16, 0, 0, 0, 400].iter().flat_map(|v: &i16| v.to_le_bytes()).collect::<Vec<u8>>();
        f.extend([0u8, 0, 0, charset, 0, 0, 0, 0]);
        f.extend([0u8; 32]);
        f
    };
    let wmf = |charset| log(Wmf::default().rec(0x02FB, &font(charset)).select(0).w(0x0521, &[1, i16::from_le_bytes([0xC0, 0]), 20, 5]));
    assert!(drawn(&wmf(204))[0].contains("\u{410}"), "{:?}", wmf(204));
    assert!(drawn(&wmf(128)).is_empty(), "no mojibake");
    // EMR_EXTCREATEFONTINDIRECTW, then EMR_EXTTEXTOUTA of one byte.
    let emf = |charset: u8| {
        let mut f = le(&[1, -12, 0, 0, 0, 400]);
        f.extend([0u8, 0, 0, charset]);
        f.extend([0u8; 4 + 64 + 28]);
        let mut t = le(&[0, 0, 0, 0, 1, 0, 0, 5, 20, 1, 76, 0, 0, 0, 0, 0, 0]);
        t.extend([0xE0u8, 0, 0, 0]);
        let mut l = Log::default();
        assert!(super::emf::play(&mut l, &Emf::default().rec(82, &f).r(37, &[1]).rec(83, &t).bytes()));
        l.ops
    };
    assert!(drawn(&emf(204))[0].contains("\u{430}"), "{:?}", emf(204));
    assert!(drawn(&emf(134)).is_empty());
    // EMR_SMALLTEXTOUT's bytes are low bytes of UTF-16 code units: Latin-1 whatever the font's character set.
    let small = |charset: u8| {
        let mut f = le(&[1, -12, 0, 0, 0, 400]);
        f.extend([0u8, 0, 0, charset]);
        f.extend([0u8; 4 + 64 + 28]);
        let mut t = le(&[5, 20, 1, 0x300, 1, 0, 0]);
        t.extend([0xE9u8, 0, 0, 0]);
        let mut l = Log::default();
        assert!(super::emf::play(&mut l, &Emf::default().rec(82, &f).r(37, &[1]).rec(108, &t).bytes()));
        l.ops
    };
    for charset in [204, 134] {
        assert!(drawn(&small(charset))[0].contains('\u{e9}'), "charset {charset}: {:?}", small(charset));
    }
}

#[test]
fn exclude_and_intersect_clip_rects_are_the_numbers_the_specification_gives() {
    let fill = |records: Wmf| pixels(records.pen(5, 0, 0).brush(0x0000FF).select(1).rect(0, 0, 100, 100));
    // META_INTERSECTCLIPRECT (0x0416) keeps the rect; META_EXCLUDECLIPRECT (0x0415) takes it out.
    let kept = fill(Wmf::default().w(0x0416, &[60, 60, 40, 40]));
    assert_eq!((kept[50 * 100 + 50], kept[10 * 100 + 10]), ([255, 0, 0, 255], [255; 4]));
    let cut = fill(Wmf::default().w(0x0415, &[60, 60, 40, 40]));
    assert_eq!((cut[50 * 100 + 50], cut[10 * 100 + 10]), ([255; 4], [255, 0, 0, 255]));
}

#[test]
fn scaling_and_blits_without_a_bitmap_read_their_records_by_the_numbers_given() {
    // META_SCALEWINDOWEXT (0x0410): the window's extent halved (1 over 2) doubles the picture's scale.
    let w = Wmf::default().w(0x0103, &[8]).w(0x020C, &[100, 100]).w(0x0410, &[2, 1, 2, 1]).pen(5, 0, 0).select(0).rect(0, 0, 50, 50);
    assert_eq!(drawn(&log(w))[0], "fill 0.0,0.0,100.0,100.0 #ffffff");
    // META_PATBLT's relative with no bitmap, META_DIBBITBLT: a reserved word before the height. Black pattern, a 50 × 50
    // square at (20, 30).
    let mut p = 0x00F0_0021u32.to_le_bytes().to_vec();
    p.extend([0i16, 0, 0, 50, 50, 30, 20].iter().flat_map(|v: &i16| v.to_le_bytes()));
    let px = pixels(Wmf::default().brush(0).select(0).rec(0x0940, &p));
    assert_eq!((px[55 * 100 + 45], px[10 * 100 + 10], px[90 * 100 + 90]), ([0, 0, 0, 255], [255; 4], [255; 4]));
}

#[test]
fn bitmaps_of_one_four_and_eight_bits_keep_their_rows_when_the_width_is_not_a_multiple_of_32() {
    let stretch = |w: i16, bmi: &[u8], bits: &[u8]| {
        let mut p = 0x00CC_0020u32.to_le_bytes().to_vec();
        p.extend([0u8, 0]);
        p.extend([2, w, 0, 0, 20, w * 10, 0, 0].iter().flat_map(|v: &i16| v.to_le_bytes()));
        p.extend(bmi);
        p.extend(bits);
        pixels(Wmf::default().rec(0x0F43, &p))
    };
    let (blue_entry, red_entry) = (|| [0xFF, 0, 0, 0], || [0, 0, 0xFF, 0]);
    let (blue, red) = (blue_entry(), red_entry());
    // 10 × 2 at one bit: 1010101010 over 1111111111 (rows bottom up, so the second listed is the top).
    let (bmi, bits) = dib(10, 2, 1, &[blue, red].concat(), &[&[0xFF, 0xC0], &[0xAA, 0x80]], 0);
    let px = stretch(10, &bmi, &bits);
    let at = |x: usize, y: usize| px[y * 100 + x * 10 + 5];
    let (red, blue) = ([255, 0, 0, 255], [0, 0, 255, 255]);
    // The top row, alternating from red; the bottom row, all red.
    assert_eq!((at(0, 5), at(1, 5), at(8, 5), at(9, 5)), (red, blue, red, blue));
    assert_eq!((at(0, 15), at(1, 15), at(8, 15), at(9, 15)), (red, red, red, red));
    // 5 × 2 at four bits: the bottom row 3 2 1 0 2, the top 0 1 2 3 1.
    let table = [blue_entry(), red_entry(), [0, 0xFF, 0, 0], [0xFF; 4]].concat();
    let (bmi, bits) = dib(5, 2, 4, &table, &[&[0x32, 0x10, 0x20], &[0x01, 0x23, 0x10]], 4);
    let px = stretch(5, &bmi, &bits);
    let at = |x: usize, y: usize| px[y * 100 + x * 10 + 5];
    assert_eq!((at(0, 15), at(2, 15), at(4, 15), at(0, 5), at(4, 5)), ([255; 4], red, [0, 255, 0, 255], blue, red));
    let (bmi, bits) = dib(3, 1, 8, &table, &[&[1, 0, 2]], 4);
    let px = stretch(3, &bmi, &bits);
    assert_eq!((px[5], px[15], px[25]), ([255, 0, 0, 255], [0, 0, 255, 255], [0, 255, 0, 255]));
}

#[test]
fn a_create_record_too_short_to_read_still_takes_its_index() {
    // A font of eight bytes, then a brush; the brush is index 1, not 0, so selecting 1 fills with it.
    let w = Wmf::default().rec(0x02FB, &[0; 8]).rec(0x00F8, &[0; 8]).brush(0xFF).pen(5, 0, 0).select(2).select(3).rect(0, 0, 50, 50);
    assert_eq!(drawn(&log(w)), ["fill 0.0,0.0,50.0,50.0 #ff0000"]);
}

#[test]
fn a_bitmap_of_three_bits_a_pixel_is_refused_and_a_pattern_brushs_usage_is_rgb() {
    let (bmi, bits) = dib(8, 1, 3, &[0; 32], &[&[0xFF; 3]], 8);
    let mut p = 0x00CC_0020u32.to_le_bytes().to_vec();
    p.extend([0u8, 0]);
    p.extend([1i16, 8, 0, 0, 20, 80, 0, 0].iter().flat_map(|v: &i16| v.to_le_bytes()));
    p.extend(&bmi);
    p.extend(&bits);
    assert!(drawn(&survives(&Wmf::default().rec(0x0F43, &p).bytes())).is_empty());
    // A BS_PATTERN DIB whose usage says palette colours still reads its own table.
    let (bmi, bits) = dib(8, 8, 8, &[0xFF, 0, 0, 0, 0, 0, 0xFF, 0], &[&[1u8; 8][..]; 8], 2);
    let mut b = [3u16, 1].iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<u8>>();
    b.extend(&bmi);
    b.extend(&bits);
    let px = pixels(Wmf::default().rec(0x0142, &b).select(0).pen(5, 0, 0).select(1).rect(0, 0, 100, 100));
    // Every texel is index 1, red, across the whole fill.
    assert!(px.iter().all(|p| p[..3] == [255, 0, 0]), "its own colours, not the text and background ones");
}
