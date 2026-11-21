//! EMF: the header, and each record read into the player.

use std::sync::Arc;

use twip_graphics::kurbo::{self, Affine, Point, Vec2};

use super::plus::Plus;
use super::raster::{NOP, Source, mask_rects};
use super::{LogBrush, LogFont, MAX_RECORDS, Object, Pen, Player, Poly, Spent, TextOut};
use crate::render::svg::Sink;

/// What an EMF's header says: the picture's size in points, the map from device pixels into it, device pixels per
/// millimetre, and the object table's size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Header {
    pub size: kurbo::Size,
    dev: Affine,
    px_per_mm: Vec2,
    handles: usize,
}

/// `ENHMETA_SIGNATURE`, " EMF".
const SIGNATURE: u32 = 0x464D_4520;

impl Header {
    /// The header of an EMF: `None` when the bytes aren't one, or its frame and bounds are empty.
    pub fn read(d: &[u8]) -> Option<Header> {
        let r = Rec(d.get(..d.len().min(108))?);
        if r.u32(0)? != 1 || r.u32(40)? != SIGNATURE {
            return None;
        }
        let size = r.u32(4)? as usize;
        if size < 88 || !size.is_multiple_of(4) || size > d.len() {
            return None;
        }
        let rect = |o| Some(kurbo::Rect::new(f64::from(r.i32(o)?), f64::from(r.i32(o + 4)?), f64::from(r.i32(o + 8)?), f64::from(r.i32(o + 12)?)));
        let (bounds, frame) = (rect(8)?, rect(24)?);
        let device = Vec2::new(f64::from(r.i32(72)?), f64::from(r.i32(76)?));
        let mut mm = Vec2::new(f64::from(r.i32(80)?), f64::from(r.i32(84)?));
        // The extension's micrometres, when the header has them, are the finer measure.
        if size >= 108 && r.u32(100)? > 0 && r.u32(104)? > 0 {
            mm = Vec2::new(f64::from(r.i32(100)?) / 1000.0, f64::from(r.i32(104)?) / 1000.0);
        }
        let handles = usize::from(r.u16(56)?);
        let to_pt = 72.0 / 2540.0;
        let (dev, size, px_per_mm) = if frame.width() > 0.0 && frame.height() > 0.0 && device.x > 0.0 && device.y > 0.0 && mm.x > 0.0 && mm.y > 0.0 {
            // Device pixels to hundredths of a millimetre, then from the frame's corner, in points.
            let dev = Affine::scale(to_pt)
                * Affine::translate((-frame.x0, -frame.y0))
                * Affine::scale_non_uniform(mm.x * 100.0 / device.x, mm.y * 100.0 / device.y);
            (dev, kurbo::Size::new(frame.width() * to_pt, frame.height() * to_pt), Vec2::new(device.x / mm.x, device.y / mm.y))
        } else if bounds.width() > 0.0 && bounds.height() > 0.0 {
            // No device metrics: the bounds at 96 pixels an inch.
            let dev = Affine::scale(0.75) * Affine::translate((-bounds.x0, -bounds.y0));
            (dev, kurbo::Size::new(bounds.width() * 0.75, bounds.height() * 0.75), Vec2::new(96.0 / 25.4, 96.0 / 25.4))
        } else {
            return None;
        };
        Some(Header { size, dev, px_per_mm, handles })
    }

    /// The frame in device pixels: what a picture drawing this one as an image counts its pixels in.
    pub(super) fn frame_px(&self) -> kurbo::Rect {
        self.dev.inverse().transform_rect_bbox(kurbo::Rect::from_origin_size(Point::ZERO, self.size))
    }
}

/// Plays the EMF `d` into `sink`, in the picture's space (its frame, in points); false when the bytes aren't an EMF.
pub fn play(sink: &mut impl Sink, d: &[u8]) -> bool {
    play_with(sink, d, Spent::default()).is_some()
}

/// [`play`], counting against what `spent` says an enclosing metafile has spent; what is spent after, or none when
/// the bytes aren't an EMF. EMF+ records in its comments play, and once they have, plain records only where the last
/// comment's records end in a `GetDC` (in a dual file the plain records are the picture again, for readers without
/// EMF+).
pub(super) fn play_with<S: Sink>(sink: &mut S, d: &[u8], spent: Spent) -> Option<Spent> {
    let h = Header::read(d)?;
    if spent.work > super::MAX_WORK {
        return Some(spent);
    }
    // EMF+ records that replace what is under them replace the picture's own, not the page's: it draws in a layer.
    // Each record looked at is work, drawn or not (a picture drawn again and again inside another reads its records
    // each time), as far as the budget left goes.
    let mut left = (super::MAX_WORK - spent.work) as usize;
    let mut isolated = false;
    for (_, r) in records(d) {
        let Some(l) = left.checked_sub(1) else { break };
        left = l;
        if let Some(plus) = plus_records(r)
            && super::plus::replaces(plus, &mut left)
        {
            isolated = true;
            break;
        }
    }
    let mut p = Player::new(sink, h.dev, h.size, h.px_per_mm, h.handles);
    p.inherit(spent);
    p.spend((super::MAX_WORK - spent.work) as usize - left);
    if isolated && p.spend_share(1.0, super::LAYER_FULL) {
        p.sink.begin_layer(1.0);
    } else {
        isolated = false;
    }
    let mut plus = Plus::default();
    for (kind, r) in records(d) {
        if !p.spend(1) {
            break;
        }
        if let Some(records) = plus_records(r) {
            plus.play(&mut p, records);
        } else if plus.plain(&mut p) {
            // A record too short for what it holds stops being read, and the next is read.
            let _ = record(&mut p, kind, r);
        }
    }
    if isolated {
        p.sink.end_layer();
    }
    Some(p.finish())
}

/// An EMF's records (their type and bytes) up to its end, at most [`MAX_RECORDS`] of them.
fn records(d: &[u8]) -> impl Iterator<Item = (u32, Rec<'_>)> {
    let mut at = 0usize;
    std::iter::from_fn(move || {
        let r = Rec(d.get(at..)?);
        let (kind, size) = (r.u32(0)?, r.u32(4)? as usize);
        if size < 8 || !size.is_multiple_of(4) || size > d.len() - at || kind == 14 {
            return None;
        }
        at += size;
        Some((kind, Rec(&d[at - size..at])))
    })
    .take(MAX_RECORDS as usize)
}

/// The EMF+ records an `EMR_COMMENT` holds, when it is an EMF+ comment.
fn plus_records(r: Rec<'_>) -> Option<&[u8]> {
    const EMF_PLUS: u32 = 0x2B46_4D45;
    if r.u32(0)? != 70 || r.u32(12)? != EMF_PLUS {
        return None;
    }
    r.slice(16, (r.u32(8)? as usize).checked_sub(4)?, 1)
}

/// A record's bytes, read little-endian; every read checked against them.
#[derive(Clone, Copy)]
pub(super) struct Rec<'a>(pub(super) &'a [u8]);

impl<'a> Rec<'a> {
    pub(super) fn bytes<const N: usize>(&self, o: usize) -> Option<[u8; N]> {
        self.0.get(o..o.checked_add(N)?)?.try_into().ok()
    }
    pub(super) fn u32(&self, o: usize) -> Option<u32> {
        self.bytes(o).map(u32::from_le_bytes)
    }
    pub(super) fn i32(&self, o: usize) -> Option<i32> {
        self.bytes(o).map(i32::from_le_bytes)
    }
    pub(super) fn u16(&self, o: usize) -> Option<u16> {
        self.bytes(o).map(u16::from_le_bytes)
    }
    pub(super) fn i16(&self, o: usize) -> Option<i16> {
        self.bytes(o).map(i16::from_le_bytes)
    }
    pub(super) fn f32(&self, o: usize) -> Option<f64> {
        self.bytes(o).map(f32::from_le_bytes).map(f64::from).filter(|v| v.is_finite())
    }
    pub(super) fn point(&self, o: usize) -> Option<Point> {
        Some(Point::new(f64::from(self.i32(o)?), f64::from(self.i32(o + 4)?)))
    }
    pub(super) fn point16(&self, o: usize) -> Option<Point> {
        Some(Point::new(f64::from(self.i16(o)?), f64::from(self.i16(o + 2)?)))
    }
    pub(super) fn size(&self, o: usize) -> Option<Vec2> {
        Some(Point::new(f64::from(self.i32(o)?), f64::from(self.i32(o + 4)?)).to_vec2())
    }
    pub(super) fn rect(&self, o: usize) -> Option<kurbo::Rect> {
        Some(kurbo::Rect::from_points(self.point(o)?, self.point(o + 8)?))
    }
    pub(super) fn xform(&self, o: usize) -> Option<Affine> {
        let a = Affine::new([self.f32(o)?, self.f32(o + 4)?, self.f32(o + 8)?, self.f32(o + 12)?, self.f32(o + 16)?, self.f32(o + 20)?]);
        a.is_finite().then_some(a)
    }
    /// `n` items of `each` bytes from `o`, when they are all inside the record.
    pub(super) fn slice(&self, o: usize, n: usize, each: usize) -> Option<&'a [u8]> {
        self.0.get(o..o.checked_add(n.checked_mul(each)?)?)
    }
    /// `n` points from `o`: 32-bit, or 16-bit when `short`.
    pub(super) fn points(&self, o: usize, n: usize, short: bool) -> Option<Vec<Point>> {
        let each = if short { 4 } else { 8 };
        self.slice(o, n, each)?;
        (0..n).map(|i| if short { self.point16(o + i * 4) } else { self.point(o + i * 8) }).collect()
    }
}

/// Where a record holds a DIB: its `BITMAPINFO` and bits by offset and size, and what its colour table holds
/// (`DIB_RGB_COLORS` 0, `DIB_PAL_COLORS` 1: indices into the selected palette, `DIB_PAL_INDICES` 2: no table, the
/// pixels themselves palette indices).
#[derive(Clone, Copy)]
pub(super) struct DibAt {
    pub(super) bmi: (u32, u32),
    pub(super) bits: (u32, u32),
    pub(super) usage: u32,
}

impl DibAt {
    /// The offsets and sizes from `bmi` (`offBmi`, `cbBmi`, `offBits`, `cbBits`), the usage at `usage`.
    pub(super) fn at(r: &Rec<'_>, bmi: usize, usage: usize) -> Option<DibAt> {
        Some(DibAt { bmi: (r.u32(bmi)?, r.u32(bmi + 4)?), bits: (r.u32(bmi + 8)?, r.u32(bmi + 12)?), usage: r.u32(usage)? })
    }
}

/// A DIB as a file the image registry reads: a BMP (or the PNG or JPEG a `BI_PNG` or `BI_JPEG` DIB holds), with its
/// height, and whether its rows run bottom up.
pub(super) struct Dib {
    pub(super) file: Arc<Vec<u8>>,
    pub(super) h: u32,
    pub(super) bottom_up: bool,
}

/// The DIB at `at` in `r` as a file, its pixels counted against the player's budget. Its colour table comes from the
/// selected palette when its usage says so. With `alpha`, a 32-bit DIB's fourth byte is premultiplied alpha
/// (`AlphaBlend`'s `AC_SRC_ALPHA`), kept. With `scans`, the bits hold only that many scans: the file is that strip,
/// and its height theirs.
pub(super) fn dib_packed<S: Sink>(p: &mut Player<'_, S>, r: Rec<'_>, at: DibAt, alpha: bool, scans: Option<u32>) -> Option<Dib> {
    let bmi = r.slice(at.bmi.0 as usize, at.bmi.1 as usize, 1)?;
    let bits = r.slice(at.bits.0 as usize, at.bits.1 as usize, 1)?;
    let h = Rec(bmi);
    let header = h.u32(0)?;
    let core = header == 12;
    let (w, signed_h, bpp, compression) = match header {
        12 => (u32::from(h.u16(4)?), i32::from(h.u16(6)?), h.u16(10)?, 0),
        40.. => (h.i32(4)?.unsigned_abs(), h.i32(8)?, h.u16(14)?, h.u32(16)?),
        _ => return None,
    };
    let (mut ht, bottom_up) = (signed_h.unsigned_abs(), signed_h > 0);
    // A PNG or JPEG counts at the size it decodes to, not the header's.
    if matches!(compression, 4 | 5) {
        let real = twip_graphics::image::probe(bits)?;
        if !p.spend_pixels(real.width, real.height) {
            return None;
        }
        return Some(Dib { file: Arc::new(bits.to_vec()), h: real.height, bottom_up: false });
    }
    if w == 0 || ht == 0 || !p.spend_pixels(w, ht) {
        return None;
    }
    let mut info = bmi.get(..header as usize)?.to_vec();
    if let Some(count) = scans {
        ht = count.min(ht);
        if ht == 0 {
            return None;
        }
        // The strip's height, its direction kept.
        if core {
            info[6..8].copy_from_slice(&(ht as u16).to_le_bytes());
        } else {
            info[8..12].copy_from_slice(&(if bottom_up { ht as i32 } else { -(ht as i32) }).to_le_bytes());
        }
    }
    let masks = if compression == 3 && header == 40 { 12 } else { 0 };
    let table_at = header as usize + masks;
    info.extend_from_slice(bmi.get(header as usize..table_at)?);
    // The colour table: as it is, or the palette's colours by index.
    match at.usage {
        1 | 2 => {
            let colours = if bpp <= 8 {
                let used = if core { 0 } else { h.u32(32)? };
                if used == 0 || used > 1 << bpp { 1usize << bpp } else { used as usize }
            } else {
                0
            };
            let palette = p.palette();
            for i in 0..colours {
                let index = if at.usage == 1 { usize::from(h.u16(table_at + i * 2)?) } else { i };
                let [r, g, b] = palette.get(index).copied().unwrap_or([0; 3]);
                info.extend_from_slice(&[b, g, r, 0][..if core { 3 } else { 4 }]);
            }
        }
        _ => info.extend_from_slice(bmi.get(table_at..)?),
    }
    let alpha = alpha && !core && bpp == 32 && compression == 0;
    if alpha {
        // A BITMAPV4HEADER whose masks name the fourth byte alpha.
        info.truncate(40);
        info[0..4].copy_from_slice(&108u32.to_le_bytes());
        info[16..20].copy_from_slice(&3u32.to_le_bytes());
        for mask in [0x00FF_0000u32, 0xFF00, 0xFF, 0xFF00_0000] {
            info.extend_from_slice(&mask.to_le_bytes());
        }
        info.resize(108, 0);
    }
    // The decoder reads three masks after the header whatever the compression: a tiny bitmap is padded for them.
    const PAD: usize = 16;
    let mut file = Vec::with_capacity(14 + info.len() + bits.len() + PAD);
    file.extend_from_slice(b"BM");
    file.extend_from_slice(&((14 + info.len() + bits.len() + PAD) as u32).to_le_bytes());
    file.extend_from_slice(&[0; 4]);
    file.extend_from_slice(&((14 + info.len()) as u32).to_le_bytes());
    file.extend_from_slice(&info);
    let pixels = file.len();
    file.extend_from_slice(bits);
    if alpha {
        for px in file[pixels..].as_chunks_mut::<4>().0 {
            let a = u32::from(px[3]);
            if a != 0 && a != 255 {
                for c in &mut px[..3] {
                    *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
                }
            }
        }
    }
    file.resize(file.len() + PAD, 0);
    Some(Dib { file: Arc::new(file), h: ht, bottom_up })
}

/// [`dib_packed`] with the rows of a bitmap of one to eight bits a pixel unpacked to 24 bits, as the image decoder
/// reads their padding wrongly.
pub(super) fn dib<S: Sink>(p: &mut Player<'_, S>, r: Rec<'_>, at: DibAt, alpha: bool, scans: Option<u32>) -> Option<Dib> {
    let d = dib_packed(p, r, at, alpha, scans)?;
    Some(Dib { file: unpack(&d.file, None)?, ..d })
}

/// The BMP `file` of an uncompressed bitmap of one to eight bits a pixel as one of 24 bits (rows in the same
/// order), its pixels through its colour table, or through `ink` (a table of two, in blue, green and red) when
/// given. Any other file as it is; `None` when the bitmap is malformed.
pub(super) fn unpack(file: &Arc<Vec<u8>>, ink: Option<[[u8; 3]; 2]>) -> Option<Arc<Vec<u8>>> {
    let d = Rec(file);
    if file.get(..2) != Some(b"BM") {
        return Some(Arc::clone(file));
    }
    let (data, header) = (d.u32(10)? as usize, d.u32(14)? as usize);
    let (w, h, bpp, compression, used, entry) = match header {
        12 => (u32::from(d.u16(18)?), i32::from(d.u16(20)?), usize::from(d.u16(24)?), 0, 0, 3),
        40.. => (d.i32(18)?.unsigned_abs(), d.i32(22)?, usize::from(d.u16(28)?), d.u32(30)?, d.u32(46)? as usize, 4),
        _ => return Some(Arc::clone(file)),
    };
    if compression != 0 || !matches!(bpp, 1 | 2 | 4 | 8) {
        // Not a depth that packs whole pixels into a byte: another depth is another decoder's, and these (3, 5, 6, 7)
        // are no bitmap's.
        return if (1..=8).contains(&bpp) { None } else { Some(Arc::clone(file)) };
    }
    let rows = h.unsigned_abs();
    twip_graphics::image::fits(w, rows).ok()?;
    let colours = if used == 0 || used > 1 << bpp { 1usize << bpp } else { used };
    let table = file.get(14 + header..)?;
    let colour = |i: usize| -> [u8; 3] {
        match ink {
            Some(t) => t.get(i).copied().unwrap_or([0; 3]),
            None => table.get(i * entry..i * entry + 3).filter(|_| i < colours).map_or([0; 3], |c| [c[0], c[1], c[2]]),
        }
    };
    let (w, rows) = (w as usize, rows as usize);
    let (stride, out_stride) = ((w * bpp).div_ceil(32) * 4, (w * 3).next_multiple_of(4));
    let bits = file.get(data..)?.get(..stride.checked_mul(rows)?)?;
    let mut out = Vec::with_capacity(54 + out_stride * rows + 16);
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&((54 + out_stride * rows + 16) as u32).to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&54u32.to_le_bytes());
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(w as i32).to_le_bytes());
    out.extend_from_slice(&h.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&((out_stride * rows) as u32).to_le_bytes());
    out.extend_from_slice(&[0; 16]);
    for row in bits.chunks_exact(stride.max(1)).take(rows) {
        for x in 0..w {
            let at = x * bpp;
            let index = usize::from(row[at / 8] >> (8 - bpp - at % 8) & ((1u16 << bpp) - 1) as u8);
            out.extend_from_slice(&colour(index));
        }
        out.resize(out.len() + out_stride - w * 3, 0);
    }
    out.resize(out.len() + 16, 0);
    Some(Arc::new(out))
}

impl Dib {
    /// A DIB record's source rect from its corner `at` and `size`, in the decoded rows (top down): a bottom-up DIB
    /// counts them from its bottom.
    pub(super) fn src(&self, at: Point, size: Vec2) -> kurbo::Rect {
        let top = if self.bottom_up { f64::from(self.h) - at.y - size.y } else { at.y };
        kurbo::Rect::new(at.x, top, at.x + size.x, top + size.y)
    }

    pub(super) fn source(&self, src: kurbo::Rect, opacity: f32) -> Source<'_> {
        Source { bitmap: &self.file, src, opacity }
    }
}

/// `SetDIBitsToDevice`: the bits at `at` hold `count` scans from `first` (from the bottom of a bottom-up DIB), and
/// what of the source rect is in them is drawn at `dst_at`, a pixel a unit. `geometry` is the destination corner,
/// source corner and size.
pub(super) fn set_dibits<S: Sink>(
    p: &mut Player<'_, S>,
    r: Rec<'_>,
    at: DibAt,
    (first, count): (u32, u32),
    geometry: (Point, Point, Vec2),
) -> Option<()> {
    let (dst_at, src_at, size) = geometry;
    let header = Rec(r.slice(at.bmi.0 as usize, at.bmi.1 as usize, 1)?);
    let full = if header.u32(0)? == 12 { u32::from(header.u16(6)?) } else { header.i32(8)?.unsigned_abs() };
    let d = dib(p, r, at, false, Some(count))?;
    // The strip's first row and the source rect, in the whole bitmap's rows top down.
    let strip = f64::from(if d.bottom_up { full.saturating_sub(first.saturating_add(d.h)) } else { first });
    let (strip_h, d) = (d.h, Dib { h: full, ..d });
    let src = d.src(src_at, size);
    let (y0, y1) = (src.y0.max(strip), src.y1.min(strip + f64::from(strip_h)));
    if y1 <= y0 || size.x == 0.0 {
        return Some(());
    }
    let dst = (Point::new(dst_at.x, dst_at.y + (y0 - src.y0)), Vec2::new(size.x, y1 - y0));
    let to = p.onto(dst);
    let src = kurbo::Rect::new(src.x0, y0 - strip, src.x1, y1 - strip);
    p.blit(0xCC, to, Some(d.source(src, 1.0)));
    Some(())
}

/// `StretchDIBits`: the DIB at `at` (none when it has no bitmap: the pattern alone) from the source rect onto the
/// destination, by the ternary raster operation `rop`.
pub(super) fn stretch_dibits<S: Sink>(p: &mut Player<'_, S>, r: Rec<'_>, at: DibAt, dst: (Point, Vec2), src: (Point, Vec2), rop: u8) -> Option<()> {
    let to = p.onto(dst);
    if at.bmi.1 == 0 {
        p.blit(rop, to, None);
        return Some(());
    }
    let d = dib(p, r, at, false, None)?;
    let src = d.src(src.0, src.1);
    p.blit(rop, to, Some(d.source(src, 1.0)));
    Some(())
}

/// A pen from a `LOGPEN`'s style and width: wider than one unit it is solid.
pub(super) fn logpen(style: u32, width: f64, colour: u32) -> Pen {
    let style = if width > 1.0 && style != super::PS_NULL { 0 } else { style };
    Pen::new(style, width, colour)
}

/// A monochrome mask DIB's size, and a test of its bit at (x, y), rows top down.
fn mask_bits<'a>(r: Rec<'a>, at: DibAt) -> Option<(u32, u32, impl Fn(u32, u32) -> bool + 'a)> {
    let bmi = Rec(r.slice(at.bmi.0 as usize, at.bmi.1 as usize, 1)?);
    let (w, h, bpp) = match bmi.u32(0)? {
        12 => (u32::from(bmi.u16(4)?), i32::from(bmi.u16(6)?), bmi.u16(10)?),
        40.. => (bmi.i32(4)?.unsigned_abs(), bmi.i32(8)?, bmi.u16(14)?),
        _ => return None,
    };
    let (rows, bottom_up) = (h.unsigned_abs(), h > 0);
    let stride = (w as usize).div_ceil(32) * 4;
    if bpp != 1 || w == 0 || rows == 0 {
        return None;
    }
    let bits = r.slice(at.bits.0 as usize, stride, rows as usize)?;
    Some((w, rows, move |x: u32, y: u32| {
        if x >= w || y >= rows {
            return false;
        }
        let row = if bottom_up { rows - 1 - y } else { y } as usize;
        bits[row * stride + x as usize / 8] >> (7 - x % 8) & 1 != 0
    }))
}

/// Characters of an `ExtTextOutA` string: Windows-1252.
pub(super) fn ansi(b: u8) -> u32 {
    narrow_char(2, b)
}

/// The character of byte `b` in code page `page` (an index into the tables).
fn narrow_char(page: usize, b: u8) -> u32 {
    if b < 0x80 { u32::from(b) } else { u32::from(super::codepages::TABLES[page][usize::from(b - 0x80)]) }
}

/// Appends the characters of the 8-bit string `bytes` in the GDI character set `charset`: the Windows code page the
/// set names, or the private-use page of a symbol font. False, appending nothing, for the double-byte sets.
// ponytail: Shift-JIS, Korean, Johab, GB 2312 and Big5 strings draw nothing (rather than the wrong characters):
// decoding them needs their double-byte tables, which the engine doesn't carry.
pub(super) fn narrow(out: &mut Vec<u32>, bytes: &[u8], charset: u8) -> bool {
    let page = match charset {
        2 => {
            out.extend(bytes.iter().map(|&b| 0xF000 + u32::from(b)));
            return true;
        }
        128..=130 | 134 | 136 => return false,
        238 => 0,
        204 => 1,
        161 => 3,
        162 => 4,
        177 => 5,
        178 => 6,
        186 => 7,
        163 => 8,
        222 => 9,
        _ => 2,
    };
    out.extend(bytes.iter().map(|&b| narrow_char(page, b)));
    true
}

/// Reads one record into the player; `None` when it is too short for what it holds.
fn record<S: Sink>(p: &mut Player<'_, S>, kind: u32, r: Rec<'_>) -> Option<()> {
    match kind {
        // EMR_POLYBEZIER, POLYGON, POLYLINE, POLYBEZIERTO, POLYLINETO and their 16-bit forms.
        2..=6 | 85..=89 => {
            let short = kind >= 85;
            let n = r.u32(24)? as usize;
            let pts = r.points(28, n, short)?;
            let poly = match if short { kind - 83 } else { kind } {
                2 => Poly::Bezier,
                3 => Poly::Gon,
                4 => Poly::Line,
                5 => Poly::BezierTo,
                _ => Poly::LineTo,
            };
            p.poly(&pts, poly);
        }
        // EMR_POLYPOLYLINE, POLYPOLYGON and their 16-bit forms.
        7 | 8 | 90 | 91 => {
            let (polys, n) = (r.u32(24)? as usize, r.u32(28)? as usize);
            let counts = r.slice(32, polys, 4)?;
            let pts = r.points(32 + polys * 4, n, kind >= 90)?;
            let counts = counts.as_chunks::<4>().0.iter().map(|c| u32::from_le_bytes(*c) as usize);
            p.poly_poly(&pts, counts, kind == 8 || kind == 91);
        }
        // EMR_POLYDRAW, POLYDRAW16
        56 | 92 => {
            let n = r.u32(24)? as usize;
            let short = kind == 92;
            let pts = r.points(28, n, short)?;
            let types = r.slice(28 + n * if short { 4 } else { 8 }, n, 1)?;
            p.poly_draw(&pts, types);
        }
        9 => p.set_ext(false, r.size(8)?),
        10 => p.dc.win_org = r.point(8)?,
        11 => p.set_ext(true, r.size(8)?),
        12 => p.dc.vp_org = r.point(8)?,
        13 => p.dc.brush_org = r.point(8)?,
        // EMR_SETPIXELV: a device pixel.
        15 => p.pixel(r.point(8)?, r.u32(16)?),
        17 => p.set_map_mode(r.u32(8)?),
        18 => p.dc.opaque = r.u32(8)? == 2,
        19 => p.dc.even_odd = r.u32(8)? != 2,
        // EMR_SETROP2: R2_BLACK (1) to R2_WHITE (16), each one more than its truth table.
        20 => {
            let r2 = r.u32(8)?;
            if (1..=16).contains(&r2) {
                p.dc.rop2 = (r2 - 1) as u8;
            }
        }
        21 => p.dc.smooth = r.u32(8)? == 4,
        22 => p.dc.text_align = r.u32(8)?,
        24 => p.dc.text_colour = r.u32(8)?,
        25 => p.dc.bk_colour = r.u32(8)?,
        26 => p.offset_clip(r.size(8)?),
        27 => p.move_to(r.point(8)?),
        // EMR_EXCLUDECLIPRECT, INTERSECTCLIPRECT
        29 | 30 => {
            let region = p.rect_path(r.rect(8)?);
            p.clip(Some((region, true)), if kind == 29 { super::RGN_DIFF } else { super::RGN_AND });
        }
        // EMR_SCALEVIEWPORTEXTEX, SCALEWINDOWEXTEX
        31 | 32 => {
            let ratio = |a: i32, b: i32| if b == 0 { 1.0 } else { f64::from(a) / f64::from(b) };
            let k = Vec2::new(ratio(r.i32(8)?, r.i32(12)?), ratio(r.i32(16)?, r.i32(20)?));
            let viewport = kind == 31;
            let e = if viewport { p.dc.vp_ext } else { p.dc.win_ext };
            p.set_ext(viewport, Vec2::new(e.x * k.x, e.y * k.y));
        }
        33 => p.save(),
        34 => p.restore(r.i32(8)?),
        35 => p.modify_world(r.xform(8)?, 4),
        36 => p.modify_world(r.xform(8)?, r.u32(32)?),
        37 => p.select(r.u32(8)?),
        // EMR_CREATEPEN: a LOGPEN. Wider than one unit, it is solid.
        38 => {
            let (style, width) = (r.u32(12)? & 0xF, f64::from(r.i32(16)?.unsigned_abs()));
            p.create(r.u32(8)?, Object::Pen(logpen(style, width, r.u32(24)?)));
        }
        // EMR_CREATEBRUSHINDIRECT: a LOGBRUSH.
        39 => {
            let (style, c, h) = (r.u32(12)?, r.u32(16)?, r.u32(20)?);
            let b = match style {
                0 => LogBrush::Solid(c),
                2 => LogBrush::Hatch(h, c),
                _ => LogBrush::Null,
            };
            p.create(r.u32(8)?, Object::Brush(b));
        }
        40 => p.delete(r.u32(8)?),
        41 => p.angle_arc(r.point(8)?, f64::from(r.u32(16)?), r.f32(20)?, r.f32(24)?),
        42 => p.ellipse(r.rect(8)?),
        43 => p.rectangle(r.rect(8)?),
        44 => p.round_rect(r.rect(8)?, r.size(24)?),
        // EMR_ARC, CHORD, PIE, ARCTO
        45..=47 | 55 => p.arc(r.rect(8)?, r.point(24)?, r.point(32)?, kind),
        // EMR_SELECTPALETTE
        48 => p.dc.palette = r.u32(8)?,
        // EMR_CREATEPALETTE: a LOGPALETTE's entries (red, green, blue, flags).
        49 => {
            let n = usize::from(r.u16(14)?);
            let entries = r.slice(16, n, 4)?;
            let colours = entries.as_chunks::<4>().0.iter().map(|e| [e[0], e[1], e[2]]).collect();
            p.create(r.u32(8)?, Object::Palette(std::rc::Rc::new(colours)));
        }
        // EMR_SETPALETTEENTRIES: from `start`, `n` entries.
        50 => {
            let (start, n) = (r.u32(12)? as usize, r.u32(16)? as usize);
            let entries = r.slice(20, n, 4)?;
            p.edit_palette(r.u32(8)?, |pal| {
                for (i, e) in entries.as_chunks::<4>().0.iter().enumerate() {
                    if let Some(c) = start.checked_add(i).and_then(|k| pal.get_mut(k)) {
                        *c = [e[0], e[1], e[2]];
                    }
                }
            });
        }
        // EMR_RESIZEPALETTE: a palette has at most 1024 entries.
        51 => {
            let n = (r.u32(12)? as usize).min(1024);
            p.edit_palette(r.u32(8)?, |pal| pal.resize(n, [0; 3]));
        }
        54 => p.line_to(r.point(8)?),
        57 => p.dc.clockwise = r.u32(8)? == 2,
        58 => {
            // A float in some writers' files, an integer in the specification's.
            let v = r.u32(8)?;
            p.dc.miter = if v > 0xFFFF { f64::from(f32::from_bits(v)) } else { f64::from(v) };
        }
        59 => p.begin_path(),
        60 => p.end_path(),
        61 => p.close_figure(),
        62 => p.draw_path(true, false),
        63 => p.draw_path(true, true),
        64 => p.draw_path(false, true),
        // EMR_FLATTENPATH: curves draw as curves.
        65 => {}
        66 => p.widen_path(),
        67 => p.clip_path(r.u32(8)?),
        68 => p.abort_path(),
        // EMR_FILLRGN, FRAMERGN, INVERTRGN, PAINTRGN
        71..=74 => {
            let len = r.u32(24)? as usize;
            let at = match kind {
                71 => 32,
                72 => 40,
                _ => 28,
            };
            let rects = region(&r, at, len)?;
            match kind {
                72 => {
                    let brush = brush_of(p, r.u32(28)?);
                    p.frame_with(&rects, &brush, f64::from(r.i32(32)?), f64::from(r.i32(36)?));
                }
                73 => {
                    let path = p.region(rects.into_iter(), false);
                    p.invert(&path);
                }
                _ => {
                    let brush = if kind == 74 { p.dc.brush.clone() } else { brush_of(p, r.u32(28)?) };
                    let path = p.region(rects.into_iter(), false);
                    p.fill_with(&path, &brush);
                }
            }
        }
        // EMR_EXTSELECTCLIPRGN: a region in device pixels; none with RGN_COPY resets the clip.
        75 => {
            let (len, mode) = (r.u32(8)? as usize, r.u32(12)?);
            let region = if len < 32 { None } else { Some((p.region(region(&r, 16, len)?.into_iter(), true), true)) };
            p.clip(region, mode);
        }
        // EMR_BITBLT, STRETCHBLT, ALPHABLEND, TRANSPARENTBLT: the source's rect mapped by its transform.
        76 | 77 | 114 | 116 => {
            let to = p.onto((r.point(24)?, r.size(32)?));
            let rop = r.u32(40)?;
            let at = DibAt::at(&r, 84, 80)?;
            if at.bmi.1 == 0 {
                if matches!(kind, 76 | 77) {
                    p.blit((rop >> 16) as u8, to, None);
                }
                return Some(());
            }
            let d = dib(p, r, at, kind == 114 && rop >> 24 == 1, None)?;
            let size = if kind == 76 { r.size(32)? } else { r.size(100)? };
            let corner = r.point(44)?;
            let src = r.xform(52)?.transform_rect_bbox(kurbo::Rect::from_points(corner, corner + size));
            match kind {
                114 => p.blit(0xCC, to, Some(d.source(src, f32::from(rop.to_le_bytes()[2]) / 255.0))),
                116 => p.keyed_blit(to, d.source(src, 1.0), rop),
                _ => p.blit((rop >> 16) as u8, to, Some(d.source(src, 1.0))),
            }
        }
        // EMR_MASKBLT: the foreground operation where the mask's bits are set, the background one where they aren't.
        78 => {
            let (dst_at, size) = (r.point(24)?, r.size(32)?);
            let to = p.onto((dst_at, size));
            let rop4 = r.u32(40)?;
            let (fore, back) = ((rop4 >> 16) as u8, (rop4 >> 24) as u8);
            let at = DibAt::at(&r, 84, 80)?;
            let d = if at.bmi.1 == 0 { None } else { Some(dib(p, r, at, false, None)?) };
            let corner = r.point(44)?;
            let src = r.xform(52)?.transform_rect_bbox(kurbo::Rect::from_points(corner, corner + size));
            let source = d.as_ref().map(|d| d.source(src, 1.0));
            let mask_at = DibAt::at(&r, 112, 108)?;
            let mask = if mask_at.bmi.1 == 0 { None } else { Some(mask(p, r, mask_at, r.point(100)?, size)?) };
            p.masked_blit(fore, back, to, source, mask.as_deref());
        }
        // EMR_PLGBLT: onto the parallelogram from its top left, top right and bottom left corners, through its mask.
        79 => {
            let [a, b, c] = [r.point(24)?, r.point(32)?, r.point(40)?];
            let m = p.m();
            let to = m * Affine::new([b.x - a.x, b.y - a.y, c.x - a.x, c.y - a.y, a.x, a.y]);
            let (corner, size) = (r.point(48)?, r.size(56)?);
            let d = dib(p, r, DibAt::at(&r, 96, 92)?, false, None)?;
            let src = r.xform(64)?.transform_rect_bbox(kurbo::Rect::from_points(corner, corner + size));
            let mask_at = DibAt::at(&r, 124, 120)?;
            let mask = if mask_at.bmi.1 == 0 { None } else { Some(mask(p, r, mask_at, r.point(112)?, size)?) };
            p.masked_blit(0xCC, NOP, to, Some(d.source(src, 1.0)), mask.as_deref());
        }
        // EMR_SETDIBITSTODEVICE: a pixel a unit.
        80 => {
            let at = DibAt::at(&r, 48, 64)?;
            set_dibits(p, r, at, (r.u32(68)?, r.u32(72)?), (r.point(24)?, r.point(32)?, r.size(40)?))?;
        }
        // EMR_STRETCHDIBITS: the source rect counts rows from the bottom of a bottom-up DIB.
        81 => {
            let at = DibAt::at(&r, 48, 64)?;
            stretch_dibits(p, r, at, (r.point(24)?, r.size(72)?), (r.point(32)?, r.size(40)?), (r.u32(68)? >> 16) as u8)?;
        }
        // EMR_EXTFLOODFILL
        // ponytail: a flood fill spreads over the pixels already drawn, which a recorded vector picture doesn't have
        // until a backend renders it, at a resolution the picture doesn't know; it draws nothing.
        53 => {}
        82 => {
            let face = r.slice(40, 32, 2)?;
            let family: String = char::decode_utf16(face.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).take_while(|&c| c != 0))
                .map(|c| c.unwrap_or('\u{FFFD}'))
                .collect();
            let f = LogFont {
                family,
                height: f64::from(r.i32(12)?),
                escapement: f64::from(r.i32(20)?),
                weight: r.i32(28)?,
                italic: r.bytes::<1>(32)?[0] != 0,
                underline: r.bytes::<1>(33)?[0] != 0,
                strike: r.bytes::<1>(34)?[0] != 0,
                charset: r.bytes::<1>(35)?[0],
            };
            p.create(r.u32(8)?, Object::Font(std::rc::Rc::new(f)));
        }
        // EMR_EXTTEXTOUTA, EXTTEXTOUTW
        83 | 84 => text(p, &r, 36, kind == 84)?,
        // EMR_POLYTEXTOUTA, POLYTEXTOUTW: each EMRTEXT 40 bytes.
        96 | 97 => {
            let n = r.u32(36)? as usize;
            r.slice(40, n, 40)?;
            for i in 0..n {
                text(p, &r, 40 + i * 40, kind == 97)?;
            }
        }
        // EMR_CREATEMONOBRUSH (its bits drawn in the text and background colours), CREATEDIBPATTERNBRUSHPT
        93 | 94 => {
            let at = DibAt::at(&r, 16, 12)?;
            let d = if kind == 93 { dib_packed(p, r, at, false, None)? } else { dib(p, r, at, false, None)? };
            p.create(r.u32(8)?, Object::Brush(LogBrush::Bitmap(d.file, kind == 93)));
        }
        // EMR_EXTCREATEPEN: a LOGPENEX with its brush (solid, hatched or a pattern) and user style's dashes.
        95 => {
            let (style, width, c) = (r.u32(28)?, f64::from(r.u32(32)?), r.u32(40)?);
            let geometric = style & 0xF_0000 != 0;
            let mut pen = Pen::new(style & 0xF, if geometric { width } else { 0.0 }, c);
            pen.cosmetic = !geometric;
            pen.brush = match r.u32(36)? {
                1 => LogBrush::Null,
                2 => LogBrush::Hatch(r.u32(44)?, c),
                3 | 5 | 6 => {
                    let at = DibAt { bmi: (r.u32(12)?, r.u32(16)?), bits: (r.u32(20)?, r.u32(24)?), usage: r.u32(44)? };
                    LogBrush::Bitmap(dib(p, r, at, false, None)?.file, false)
                }
                _ => LogBrush::Solid(c),
            };
            pen.cap = match style & 0xF00 {
                0x100 => kurbo::Cap::Square,
                0x200 => kurbo::Cap::Butt,
                _ => kurbo::Cap::Round,
            };
            pen.join = match style & 0xF000 {
                0x1000 => kurbo::Join::Bevel,
                0x2000 => kurbo::Join::Miter,
                _ => kurbo::Join::Round,
            };
            if pen.style == super::PS_USERSTYLE {
                let n = (r.u32(48)? as usize).min(16);
                let dashes: Option<Vec<f64>> = (0..n).map(|i| r.u32(52 + i * 4).map(f64::from)).collect();
                pen.dashes = dashes?.into();
                if geometric && width > 0.0 {
                    // Geometric dashes are in logical units; the stroke scales them by its width.
                    pen.dashes = pen.dashes.iter().map(|d| d / width).collect();
                }
            }
            p.create(r.u32(8)?, Object::Pen(pen));
        }
        // EMR_SMALLTEXTOUT: no spacing; the rectangle only without ETO_NO_RECT; 8-bit characters with ETO_SMALL_CHARS.
        108 => {
            let (at, n, options) = (r.point(8)?, r.u32(16)? as usize, r.u32(20)?);
            let start = if options & 0x100 != 0 { 36 } else { 52 };
            let rect = if options & 0x100 != 0 { None } else { r.rect(36) };
            let mut chars = std::mem::take(&mut p.chars);
            chars.clear();
            if options & 0x200 != 0 {
                // Each byte is the low byte of a UTF-16 code unit, whatever the font's character set.
                chars.extend(r.slice(start, n, 1)?.iter().map(|&b| u32::from(b)));
            } else {
                utf16(&mut chars, r.slice(start, n, 2)?);
            }
            let out = TextOut {
                at,
                chars: &chars,
                glyph_ids: options & 0x10 != 0,
                advances: None,
                rises: None,
                extra: (0.0, 0.0),
                rect,
                opaque: options & 2 != 0,
                clipped: options & 4 != 0,
            };
            p.text(out);
            p.chars = chars;
        }
        // EMR_GRADIENTFILL: vertices, then rects or triangles by vertex index.
        118 => {
            let (nv, nt, mode) = (r.u32(24)? as usize, r.u32(28)? as usize, r.u32(32)?);
            let verts = r.slice(36, nv, 16)?;
            let vertex = |i: u32| {
                let at = (i as usize).checked_mul(16)?;
                let v = Rec(verts.get(at..at.checked_add(16)?)?);
                let ch = |o| Some(f32::from(v.u16(o)? >> 8) / 255.0);
                Some((v.point(0)?, [ch(8)?, ch(10)?, ch(12)?, 1.0]))
            };
            let at = 36 + nv * 16;
            if mode == 2 {
                let tris = r.slice(at, nt, 12)?;
                for t in tris.as_chunks::<12>().0 {
                    let i = |k: usize| u32::from_le_bytes([t[k], t[k + 1], t[k + 2], t[k + 3]]);
                    let (Some(a), Some(b), Some(c)) = (vertex(i(0)), vertex(i(4)), vertex(i(8))) else { continue };
                    p.gradient_triangle([a.0, b.0, c.0], [a.1, b.1, c.1]);
                }
            } else {
                let rects = r.slice(at, nt, 8)?;
                for g in rects.as_chunks::<8>().0 {
                    let i = |k: usize| u32::from_le_bytes([g[k], g[k + 1], g[k + 2], g[k + 3]]);
                    let (Some(a), Some(b)) = (vertex(i(0)), vertex(i(4))) else { continue };
                    p.gradient_rect(a.0, b.0, a.1, b.1, mode == 1);
                }
            }
        }
        // The header, comments (EMF+ among them), palettes, colour management, layout and every other record that
        // draws nothing here.
        _ => {}
    }
    Some(())
}

/// The brush at handle `i`, or none.
fn brush_of<S: Sink>(p: &Player<'_, S>, i: u32) -> LogBrush {
    match p.object(i) {
        Some(Object::Brush(b)) => b,
        _ => LogBrush::Null,
    }
}

/// A mask DIB's bits from `corner`, `size` of them, as rects in the unit square.
fn mask<S: Sink>(p: &mut Player<'_, S>, r: Rec<'_>, at: DibAt, corner: Point, size: Vec2) -> Option<Vec<kurbo::Rect>> {
    let (_, _, bits) = mask_bits(r, at)?;
    let (x, y) = (corner.x.max(0.0) as u32, corner.y.max(0.0) as u32);
    let (w, h) = (size.x.abs().min(f64::from(1u32 << 16)) as u32, size.y.abs().min(f64::from(1u32 << 16)) as u32);
    if !p.spend_pixels(w, h) {
        return None;
    }
    let rects = mask_rects(&bits, (x, y, w, h), &mut |n| p.spend(n));
    p.spend(rects.len()).then_some(rects)
}

/// UTF-16LE code units as code points; unpaired surrogates become U+FFFD.
pub(super) fn utf16(out: &mut Vec<u32>, b: &[u8]) {
    out.extend(char::decode_utf16(b.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c))).map(|c| c.map_or(0xFFFD, u32::from)));
}

/// A `RGNDATA` of `len` bytes at `at`: its rects.
fn region(r: &Rec<'_>, at: usize, len: usize) -> Option<Vec<kurbo::Rect>> {
    let data = Rec(r.slice(at, len, 1)?);
    let n = data.u32(8)? as usize;
    let rects = data.slice(32, n, 16)?;
    Some(rects.as_chunks::<16>().0.iter().filter_map(|c| Rec(c).rect(0)).collect())
}

/// The `EMRTEXT` at `at` in `r`, with its string (UTF-16 when `wide`) and spacing, drawn.
fn text<S: Sink>(p: &mut Player<'_, S>, r: &Rec<'_>, at: usize, wide: bool) -> Option<()> {
    let e = |o: usize| r.u32(at + o);
    let reference = r.point(at)?;
    let (n, off_string, options, off_dx) = (e(8)? as usize, e(12)? as usize, e(16)?, e(36)? as usize);
    let rect = r.rect(at + 20)?;
    let mut chars = std::mem::take(&mut p.chars);
    chars.clear();
    if wide {
        utf16(&mut chars, r.slice(off_string, n, 2)?);
    } else {
        if !narrow(&mut chars, r.slice(off_string, n, 1)?, p.dc.font.charset) {
            p.chars = chars;
            return Some(());
        }
    }
    // With ETO_PDY, an x and a y for each character.
    let pairs = options & 0x2000 != 0;
    let (mut dx, mut dy) = (std::mem::take(&mut p.advances_in), std::mem::take(&mut p.rises_in));
    dx.clear();
    dy.clear();
    if off_dx != 0
        && let Some(s) = r.slice(off_dx, n, if pairs { 8 } else { 4 })
    {
        for c in s.as_chunks::<4>().0.iter().map(|c| f64::from(i32::from_le_bytes(*c))).enumerate() {
            if pairs && c.0 % 2 == 1 { dy.push(c.1) } else { dx.push(c.1) }
        }
    }
    let out = TextOut {
        at: reference,
        chars: &chars,
        glyph_ids: options & 0x10 != 0,
        advances: (dx.len() == chars.len() && !dx.is_empty()).then_some(&dx[..]),
        rises: (dy.len() == chars.len() && !dy.is_empty()).then_some(&dy[..]),
        extra: (0.0, 0.0),
        rect: (rect.width() != 0.0 && rect.height() != 0.0).then_some(rect),
        opaque: options & 2 != 0,
        clipped: options & 4 != 0,
    };
    p.text(out);
    p.chars = chars;
    p.advances_in = dx;
    p.rises_in = dy;
    Some(())
}
