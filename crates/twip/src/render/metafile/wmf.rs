//! WMF: the header, and each record read into the player. A WMF's device is a 96 dpi screen: a placeable file's
//! bounding box is its window, mapped onto a viewport of the box's size in inches, and a file without one is sized by
//! its window extent, a unit a pixel.

use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

use twip_graphics::kurbo::{self, Affine, Point, Vec2};

use super::emf::{self, DibAt, Rec, ansi, dib, dib_packed, narrow, set_dibits, stretch_dibits};
use super::{LogBrush, LogFont, MAX_RECORDS, Object, Player, Poly, Spent, TextOut, emf::logpen};
use crate::render::svg::Sink;

/// The key a placeable file starts with.
const PLACEABLE: u32 = 0x9AC6_CDD7;
/// A placeable header's size, and the standard header's.
const PLACEABLE_SIZE: usize = 22;
const HEADER_SIZE: usize = 18;
/// The most objects a metafile holds at once (an object index is 16 bits).
const MAX_OBJECTS: u32 = 1 << 16;
/// The most bytes of embedded EMF reassembled from a file's escape records.
const MAX_EMBEDDED: usize = 1 << 26;

/// What a WMF's header says: the picture's size in points, and the window and viewport its records start from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Header {
    pub size: kurbo::Size,
    first: usize,
    win_org: Point,
    win_ext: Vec2,
    vp_ext: Vec2,
}

impl Header {
    /// The header of a WMF: `None` when the bytes aren't one, or have no extent to draw in.
    pub fn read(d: &[u8]) -> Option<Header> {
        let r = Rec(d);
        let placeable = r.u32(0)? == PLACEABLE;
        let at = if placeable { PLACEABLE_SIZE } else { 0 };
        let h = Rec(d.get(at..at + HEADER_SIZE)?);
        // Memory or disk file, a nine word header, version 1 or 3.
        if !matches!(h.u16(0)?, 1 | 2) || h.u16(2)? != 9 || !matches!(h.u16(4)?, 0x100 | 0x300) {
            return None;
        }
        let first = at + HEADER_SIZE;
        let (org, ext, px) = if placeable {
            let v = |o| r.i16(o).map(f64::from);
            let (l, t, rt, b) = (v(6)?, v(8)?, v(10)?, v(12)?);
            let inch = r.u16(14)?;
            (Point::new(l, t), Vec2::new(rt - l, b - t), 96.0 / f64::from(if inch == 0 { 1440 } else { inch }))
        } else {
            let (org, ext) = window(d.get(first..)?)?;
            (org, ext, 1.0)
        };
        let vp_ext = Vec2::new(ext.x.abs() * px, ext.y.abs() * px);
        if !(vp_ext.x > 0.0 && vp_ext.y > 0.0) {
            return None;
        }
        Some(Header { size: kurbo::Size::new(vp_ext.x * 0.75, vp_ext.y * 0.75), first, win_org: org, win_ext: ext, vp_ext })
    }
}

/// The window a file without a placeable header sets: its origin and extent, from the first extent record.
fn window(records: &[u8]) -> Option<(Point, Vec2)> {
    let mut org = Point::ZERO;
    let (mut at, mut n) = (0usize, 0u32);
    while n < MAX_RECORDS {
        let (r, size) = next(records, at)?;
        let (y, x) = (f64::from(r.i16(6).unwrap_or(0)), f64::from(r.i16(8).unwrap_or(0)));
        match r.u16(4)? {
            0x020B => org = Point::new(x, y),
            0x020C => return Some((org, Vec2::new(x, y))),
            0 => return None,
            _ => {}
        }
        at += size;
        n += 1;
    }
    None
}

/// The record at `at` in `d` and its size in bytes; `None` at the end, or when it doesn't fit the data.
fn next(d: &[u8], at: usize) -> Option<(Rec<'_>, usize)> {
    let rest = d.get(at..)?;
    let words = Rec(rest).u32(0)? as usize;
    let size = words.checked_mul(2)?;
    (words >= 3 && size <= rest.len()).then(|| (Rec(&rest[..size]), size))
}

/// Plays the WMF `d` into `sink`, in the picture's space; false when the bytes aren't a WMF.
pub fn play(sink: &mut impl Sink, d: &[u8]) -> bool {
    play_with(sink, d, Spent::default()).is_some()
}

/// [`play`], counting against what `spent` says an enclosing metafile has spent; what is spent after, or none when
/// the bytes aren't a WMF.
pub(super) fn play_with<S: Sink>(sink: &mut S, d: &[u8], spent: Spent) -> Option<Spent> {
    let h = Header::read(d)?;
    // A file that carries its picture as an EMF is that picture, and its own records (a stand-in for those that can't
    // play the EMF) don't draw beside it.
    if let Some(data) = embedded_emf(d, h.first)
        && let Some(e) = emf::Header::read(&data)
    {
        sink.save();
        sink.transform(Affine::scale_non_uniform(h.size.width / e.size.width, h.size.height / e.size.height));
        let spent = emf::play_with(sink, &data, spent);
        sink.restore();
        return spent;
    }
    let mut p = Player::new(sink, Affine::scale(0.75), h.size, Vec2::new(96.0 / 25.4, 96.0 / 25.4), MAX_OBJECTS as usize + 1);
    p.inherit(spent);
    p.dc.map_mode = super::MM_ANISOTROPIC;
    (p.dc.win_org, p.dc.win_ext, p.dc.vp_ext) = (h.win_org, h.win_ext, h.vp_ext);
    let mut objects = Objects::default();
    let (mut at, mut n) = (h.first, 0u32);
    while p.spend(1) && n < MAX_RECORDS {
        let Some((r, size)) = next(d, at) else { break };
        let Some(kind) = r.u16(4) else { break };
        if kind == 0 {
            break;
        }
        // A record too short for what it holds stops being read, and the next is read.
        let _ = record(&mut p, &mut objects, kind, r);
        at += size;
        n += 1;
    }
    Some(p.finish())
}

/// The state a WMF has beyond the player's: the object table, in which a created object takes the lowest free index
/// and a delete frees it (regions live here, as the player's objects have none), and the text spacing (saved and
/// restored with the player's states).
#[derive(Default)]
struct Objects {
    free: BTreeSet<u32>,
    used: u32,
    regions: HashMap<u32, Rc<Vec<kurbo::Rect>>>,
    now: Local,
    saved: Vec<Local>,
    /// Saves past [`MAX_SAVES`](super::MAX_SAVES), as the player counts them.
    overflow: usize,
}

/// What a saved state keeps of a WMF's own state.
#[derive(Clone, Copy, Default)]
struct Local {
    /// `SetTextCharExtra`, and `SetTextJustification`'s extra and break count.
    char_extra: f64,
    break_extra: f64,
    break_count: f64,
}

impl Objects {
    /// The lowest free index, its object emptied; `None` with every one taken.
    fn alloc<S: Sink>(&mut self, p: &mut Player<'_, S>) -> Option<u32> {
        let i = match self.free.pop_first() {
            Some(i) => i,
            None if self.used < MAX_OBJECTS => {
                self.used += 1;
                self.used - 1
            }
            None => return None,
        };
        p.delete(i + 1);
        self.regions.remove(&i);
        Some(i)
    }

    fn delete<S: Sink>(&mut self, p: &mut Player<'_, S>, i: u32) {
        if i < self.used && self.free.insert(i) {
            p.delete(i + 1);
            self.regions.remove(&i);
        }
    }

    /// `SaveDC`, as the player counts it.
    fn save(&mut self) {
        if self.saved.len() < super::MAX_SAVES {
            self.saved.push(self.now);
        } else {
            self.overflow += 1;
        }
    }

    /// `RestoreDC`, as the player counts it.
    fn restore(&mut self, n: i32) {
        let depth = self.saved.len() + self.overflow;
        let levels = if n < 0 { n.unsigned_abs() as usize } else { (depth + 1).saturating_sub(n as usize) };
        if n == 0 || levels == 0 || levels > depth {
            return;
        }
        let from_overflow = levels.min(self.overflow);
        self.overflow -= from_overflow;
        let rest = levels - from_overflow;
        if rest > 0 {
            let keep = self.saved.len() - rest;
            self.saved.truncate(keep + 1);
            if let Some(l) = self.saved.pop() {
                self.now = l;
            }
        }
    }

    /// The extra advance after each character and each space, in logical units.
    fn extra(&self) -> (f64, f64) {
        let l = &self.now;
        (l.char_extra, if l.break_count > 0.0 { l.break_extra / l.break_count } else { 0.0 })
    }
}

/// Reads one record into the player; `None` when it is too short for what it holds.
fn record<S: Sink>(p: &mut Player<'_, S>, t: &mut Objects, kind: u16, r: Rec<'_>) -> Option<()> {
    // Parameters by their word: the arguments of the call, in reverse.
    let u = |k: usize| r.u16(6 + 2 * k);
    let s = |k: usize| r.i16(6 + 2 * k);
    let dw = |k: usize| r.u32(6 + 2 * k);
    let f = |k: usize| s(k).map(f64::from);
    let pt = |k: usize| Some(Point::new(f(k + 1)?, f(k)?));
    let rect = |k: usize| Some(kurbo::Rect::from_points(Point::new(f(k + 3)?, f(k + 2)?), Point::new(f(k + 1)?, f(k)?)));
    match kind {
        0x0103 => p.set_map_mode(u32::from(u(0)?)),
        0x0102 => p.dc.opaque = u(0)? == 2,
        0x0106 => p.dc.even_odd = u(0)? != 2,
        // SETROP2: R2_BLACK (1) to R2_WHITE (16).
        0x0104 => {
            let r2 = u(0)?;
            if (1..=16).contains(&r2) {
                p.dc.rop2 = (r2 - 1) as u8;
            }
        }
        0x0107 => p.dc.smooth = u(0)? == 4,
        0x012E => p.dc.text_align = u32::from(u(0)?),
        0x0209 => p.dc.text_colour = dw(0)?,
        0x0201 => p.dc.bk_colour = dw(0)?,
        0x020B => p.dc.win_org = pt(0)?,
        0x020C => p.set_ext(false, pt(0)?.to_vec2()),
        0x020D => p.dc.vp_org = pt(0)?,
        0x020E => p.set_ext(true, pt(0)?.to_vec2()),
        0x020F => p.dc.win_org += pt(0)?.to_vec2(),
        0x0211 => p.dc.vp_org += pt(0)?.to_vec2(),
        // SCALEWINDOWEXT, SCALEVIEWPORTEXT: each side's extent by a numerator over a denominator.
        0x0410 | 0x0412 => {
            let ratio = |num: f64, den: f64| if den == 0.0 { 1.0 } else { num / den };
            let k = Vec2::new(ratio(f(3)?, f(2)?), ratio(f(1)?, f(0)?));
            let viewport = kind == 0x0412;
            let e = if viewport { p.dc.vp_ext } else { p.dc.win_ext };
            p.set_ext(viewport, Vec2::new(e.x * k.x, e.y * k.y));
        }
        0x0213 => p.line_to(pt(0)?),
        0x0214 => p.move_to(pt(0)?),
        0x0220 => p.offset_clip(pt(0)?.to_vec2()),
        // EXCLUDECLIPRECT, INTERSECTCLIPRECT
        0x0415 | 0x0416 => {
            let region = p.rect_path(rect(0)?);
            p.clip(Some((region, true)), if kind == 0x0415 { super::RGN_DIFF } else { super::RGN_AND });
        }
        // SELECTCLIPREGION: a region, or none to reset the clip.
        0x012C => {
            let region = t.regions.get(&u32::from(u(0)?)).map(|rects| (p.region(rects.iter().copied(), false), true));
            p.clip(region, super::RGN_COPY);
        }
        0x001E => {
            p.save();
            t.save();
        }
        0x0127 => {
            p.restore(i32::from(s(0)?));
            t.restore(i32::from(s(0)?));
        }
        0x012D => p.select(u32::from(u(0)?) + 1),
        0x0108 => t.now.char_extra = f(0)?,
        0x020A => (t.now.break_count, t.now.break_extra) = (f(0)?, f(1)?),
        0x01F0 => t.delete(p, u32::from(u(0)?)),
        // The records that create an object, which each take an index even when they hold nothing to draw by.
        0x02FA | 0x02FB | 0x02FC | 0x00F7 | 0x0142 | 0x01F9 | 0x06FF | 0x02FD | 0x06FE | 0x00F8 => {
            let made = made(p, kind, r);
            let i = t.alloc(p)?;
            match made {
                Some(Made::Object(o)) => p.create(i + 1, o),
                Some(Made::Region(rects)) => {
                    t.regions.insert(i, Rc::new(rects));
                }
                None => {}
            }
        }
        0x0234 => p.dc.palette = u32::from(u(0)?) + 1,
        // SETPALENTRIES: from `start`, the selected palette's entries.
        0x0037 => {
            let (start, n) = (usize::from(u(0)?), usize::from(u(1)?));
            let entries = r.slice(10, n, 4)?;
            p.edit_palette(p.dc.palette, |pal| {
                for (i, e) in entries.as_chunks::<4>().0.iter().enumerate() {
                    if let Some(c) = pal.get_mut(start + i) {
                        *c = [e[0], e[1], e[2]];
                    }
                }
            });
        }
        0x0139 => {
            let n = usize::from(u(0)?).min(1024);
            p.edit_palette(p.dc.palette, |pal| pal.resize(n, [0; 3]));
        }
        // POLYGON, POLYLINE
        0x0324 | 0x0325 => {
            let pts = r.points(8, usize::from(u(0)?), true)?;
            p.poly(&pts, if kind == 0x0324 { Poly::Gon } else { Poly::Line });
        }
        0x0538 => {
            let n = usize::from(u(0)?);
            let counts: Vec<usize> = r.slice(8, n, 2)?.as_chunks::<2>().0.iter().map(|c| usize::from(u16::from_le_bytes(*c))).collect();
            let pts = r.points(8 + n * 2, counts.iter().sum(), true)?;
            p.poly_poly(&pts, counts.into_iter(), true);
        }
        0x041B => p.rectangle(rect(0)?),
        0x0418 => p.ellipse(rect(0)?),
        0x061C => p.round_rect(rect(2)?, Vec2::new(f(1)?, f(0)?)),
        // ARC, CHORD, PIE: the rect, then the arc's start and end points.
        0x0817 | 0x0830 | 0x081A => {
            let shape = match kind {
                0x0817 => 45,
                0x0830 => 46,
                _ => 47,
            };
            p.arc(rect(4)?, pt(2)?, pt(0)?, shape);
        }
        0x041F => p.pixel(pt(2)?, dw(0)?),
        // PATBLT: the brush's pattern by the raster operation.
        0x061D => {
            let to = p.onto((pt(4)?, Vec2::new(f(3)?, f(2)?)));
            p.blit((dw(0)? >> 16) as u8, to, None);
        }
        0x0521 => {
            let n = usize::from(u(0)?);
            let end = 8 + n.next_multiple_of(2);
            text(p, t, &r, Line { at: Point::new(coord(&r, end + 2)?, coord(&r, end)?), n, string: 8, ..Line::default() })?;
        }
        0x0A32 => {
            let (n, options) = (usize::from(u(2)?), u(3)?);
            let boxed = options & 6 != 0;
            let string = if boxed { 22 } else { 14 };
            let rect = if boxed { r.rect16(14) } else { None };
            let dx = string + n.next_multiple_of(2);
            text(p, t, &r, Line { at: pt(0)?, n, string, rect, options, dx: Some(dx) })?;
        }
        // PAINTREGION, INVERTREGION, FILLREGION, FRAMEREGION
        0x012B | 0x012A | 0x0228 | 0x0429 => {
            let rects = t.regions.get(&u32::from(u(0)?))?.clone();
            let path = p.region(rects.iter().copied(), false);
            match kind {
                0x012B => {
                    let brush = p.dc.brush.clone();
                    p.fill_with(&path, &brush);
                }
                0x012A => p.invert(&path),
                0x0228 => {
                    let brush = brush_at(p, u32::from(u(1)?));
                    p.fill_with(&path, &brush);
                }
                _ => {
                    let brush = brush_at(p, u32::from(u(1)?));
                    p.frame_with(&rects, &brush, f(3)?, f(2)?);
                }
            }
        }
        // STRETCHDIB: the source rect counts rows from the bottom of a bottom-up DIB.
        0x0F43 => {
            let at = packed(r, 28, u32::from(u(2)?))?;
            stretch_dibits(p, r, at, (pt(9)?, Vec2::new(f(8)?, f(7)?)), (pt(5)?, Vec2::new(f(4)?, f(3)?)), (dw(0)? >> 16) as u8)?;
        }
        // DIBSTRETCHBLT, DIBBITBLT (a DIB), STRETCHBLT, BITBLT (a device bitmap). A record the size of its fixed
        // fields has no bitmap and a reserved word before the destination's height: the pattern alone.
        0x0B41 | 0x0940 | 0x0B23 | 0x0922 => {
            let stretch = matches!(kind, 0x0B41 | 0x0B23);
            let none = r.0.len() / 2 == usize::from(kind >> 8) + 3;
            let k = usize::from(none);
            let (dst, src, size, bits) = if stretch {
                (pt(8 + k)?, pt(4)?, Vec2::new(f(7 + k)?, f(6 + k)?), 26)
            } else {
                (pt(6 + k)?, pt(2)?, Vec2::new(f(5 + k)?, f(4 + k)?), 22)
            };
            let src_size = if stretch { Vec2::new(f(3)?, f(2)?) } else { size };
            let rop = (dw(0)? >> 16) as u8;
            if none {
                p.blit(rop, p.onto((dst, size)), None);
            } else if matches!(kind, 0x0B41 | 0x0940) {
                stretch_dibits(p, r, packed(r, bits, 0)?, (dst, size), (src, src_size), rop)?;
            } else {
                let (d, _) = device_bitmap(p, r, bits, bits + 10, false)?;
                let from = d.src(src, src_size);
                p.blit(rop, p.onto((dst, size)), Some(d.source(from, 1.0)));
            }
        }
        // SETDIBTODEV: a pixel a unit.
        0x0D33 => {
            let at = packed(r, 24, u32::from(u(0)?))?;
            set_dibits(p, r, at, (u32::from(u(2)?), u32::from(u(1)?)), (pt(7)?, pt(3)?, Vec2::new(f(6)?, f(5)?)))?;
        }
        // Flood fills, palette realization and animation, escapes (an EMF a file carries plays instead of the file)
        // and every other record that draws nothing here.
        _ => {}
    }
    Some(())
}

/// A 16-bit coordinate as a float.
fn coord(r: &Rec<'_>, o: usize) -> Option<f64> {
    r.i16(o).map(f64::from)
}

impl Rec<'_> {
    /// A rect of four 16-bit coordinates (left, top, right, bottom) from `o`.
    fn rect16(&self, o: usize) -> Option<kurbo::Rect> {
        let v = |k: usize| coord(self, o + 2 * k);
        Some(kurbo::Rect::from_points(Point::new(v(0)?, v(1)?), Point::new(v(2)?, v(3)?)))
    }
}

/// The brush at index `i`, or none.
fn brush_at<S: Sink>(p: &Player<'_, S>, i: u32) -> LogBrush {
    match p.object(i + 1) {
        Some(Object::Brush(b)) => b,
        _ => LogBrush::Null,
    }
}

/// What a create record makes.
enum Made {
    Object(Object),
    Region(Vec<kurbo::Rect>),
}

/// The object a create record `kind` holds; `None` when it holds nothing the player draws by, or is too short for what
/// it holds (its index is taken all the same).
fn made<S: Sink>(p: &mut Player<'_, S>, kind: u16, r: Rec<'_>) -> Option<Made> {
    let u = |k: usize| r.u16(6 + 2 * k);
    let s = |k: usize| r.i16(6 + 2 * k);
    let dw = |k: usize| r.u32(6 + 2 * k);
    let f = |k: usize| s(k).map(f64::from);
    Some(Made::Object(match kind {
        // CREATEPENINDIRECT: a LOGPEN.
        0x02FA => Object::Pen(logpen(u32::from(u(0)?) & 0xF, f64::from(s(1)?.unsigned_abs()), dw(3)?)),
        // CREATEBRUSHINDIRECT: a LOGBRUSH.
        0x02FC => {
            let (style, c, h) = (u(0)?, dw(1)?, u32::from(u(3)?));
            Object::Brush(match style {
                0 => LogBrush::Solid(c),
                2 => LogBrush::Hatch(h, c),
                _ => LogBrush::Null,
            })
        }
        // CREATEFONTINDIRECT: a LOGFONT, its face name up to 32 bytes.
        0x02FB => {
            let face = r.0.get(24..r.0.len().min(56))?;
            let family: String = face.iter().take_while(|&&b| b != 0).map(|&b| char::from_u32(ansi(b)).unwrap_or('\u{FFFD}')).collect();
            let flag = |o: usize| r.bytes::<1>(o).map(|b| b[0] != 0);
            Object::Font(Rc::new(LogFont {
                family,
                height: f(0)?,
                escapement: f(2)?,
                weight: i32::from(s(4)?),
                italic: flag(16)?,
                underline: flag(17)?,
                strike: flag(18)?,
                charset: r.bytes::<1>(19)?[0],
            }))
        }
        // CREATEPALETTE: a LOGPALETTE's entries (red, green, blue, flags).
        0x00F7 => {
            let entries = r.slice(10, usize::from(u(1)?), 4)?;
            Object::Palette(Rc::new(entries.as_chunks::<4>().0.iter().map(|e| [e[0], e[1], e[2]]).collect()))
        }
        // DIBCREATEPATTERNBRUSH: a DIB; of a pattern brush (`BS_PATTERN`), a monochrome one draws in the text and
        // background colours.
        0x0142 => {
            let style = u(0)?;
            // A pattern brush's colours are in its DIB: the usage is RGB colours whatever the record says.
            let usage = if style == 3 { 0 } else { u32::from(u(1)?) };
            let at = packed(r, 10, usage)?;
            let bpp = if r.u32(10)? == 12 { r.u16(20)? } else { r.u16(24)? };
            let mono = style == 3 && bpp == 1;
            let file = if mono { dib_packed(p, r, at, false, None)?.file } else { dib(p, r, at, false, None)?.file };
            Object::Brush(LogBrush::Bitmap(file, mono))
        }
        // CREATEPATTERNBRUSH: a device bitmap, after 18 reserved bytes (a monochrome one draws in the text and
        // background colours).
        0x01F9 => {
            let (d, mono) = device_bitmap(p, r, 6, 38, true)?;
            Object::Brush(LogBrush::Bitmap(d.file, mono))
        }
        // CREATEREGION: scans of left and right pairs.
        0x06FF => return scans(&r).map(Made::Region),
        // CREATEBITMAP, CREATEBITMAPINDIRECT, and the CREATEBRUSH no specification defines: an index each.
        _ => return None,
    }))
}

/// Where the packed DIB at byte `at` of a record has its header, colour table and bits; `None` without one.
fn packed(r: Rec<'_>, at: usize, usage: u32) -> Option<DibAt> {
    let header = r.u32(at)? as usize;
    let (bpp, used, compression, entry) = match header {
        12 => (usize::from(r.u16(at + 10)?), 0, 0, 3),
        40.. => (usize::from(r.u16(at + 14)?), r.u32(at + 32)? as usize, r.u32(at + 16)?, if usage == 1 { 2 } else { 4 }),
        _ => return None,
    };
    // A table of biClrUsed entries at any depth; one of the depth's full size at eight bits and under when it says
    // none; none in a PNG or a JPEG.
    let colours = match bpp {
        _ if matches!(compression, 4 | 5) => 0,
        0..=8 if used == 0 || used > 1 << bpp => 1 << bpp,
        _ => used,
    };
    let masks = if compression == 3 && header == 40 { 12 } else { 0 };
    let bits = at.checked_add(header)?.checked_add(masks)?.checked_add(colours.checked_mul(entry)?)?;
    let len = r.0.len().checked_sub(bits)?;
    Some(DibAt { bmi: (at as u32, (bits - at) as u32), bits: (bits as u32, len as u32), usage })
}

/// The device-dependent bitmap (`Bitmap16`) with its header at byte `at` and its bits at `bits_at`, as a DIB, and
/// whether it is monochrome: its one bits are the background colour, its zero bits the text colour. A bitmap of
/// four or eight bits a pixel indexes the selected palette.
fn device_bitmap<S: Sink>(p: &mut Player<'_, S>, r: Rec<'_>, at: usize, bits_at: usize, pattern: bool) -> Option<(super::emf::Dib, bool)> {
    let (w, h, row_bytes) = (usize::try_from(r.i16(at + 2)?).ok()?, usize::try_from(r.i16(at + 4)?).ok()?, usize::try_from(r.i16(at + 6)?).ok()?);
    let (planes, bpp) = (r.bytes::<1>(at + 8)?[0], usize::from(r.bytes::<1>(at + 9)?[0]));
    if w == 0 || h == 0 || planes != 1 || !matches!(bpp, 1 | 4 | 8 | 16 | 24 | 32) {
        return None;
    }
    let row = (w * bpp).div_ceil(8);
    if row_bytes < row {
        return None;
    }
    let bits = r.slice(bits_at, h, row_bytes)?;
    let stride = row.next_multiple_of(4);
    let indexed = matches!(bpp, 4 | 8);
    let mut file = Vec::with_capacity(48 + stride * h);
    let mut head = |v: &[u8]| file.extend_from_slice(v);
    head(&40u32.to_le_bytes());
    head(&(w as i32).to_le_bytes());
    // Top down.
    head(&(-(h as i32)).to_le_bytes());
    head(&1u16.to_le_bytes());
    head(&(bpp as u16).to_le_bytes());
    head(&[0; 4]);
    head(&((stride * h) as u32).to_le_bytes());
    head(&[0; 8]);
    head(
        &(if bpp == 1 {
            2u32
        } else if indexed {
            1 << bpp
        } else {
            0
        })
        .to_le_bytes(),
    );
    head(&[0; 4]);
    if bpp == 1 {
        for c in [p.dc.text_colour, p.dc.bk_colour] {
            let [r, g, b, _] = p.rgb(c).map(|v| (v * 255.0).round() as u8);
            file.extend_from_slice(&[b, g, r, 0]);
        }
    }
    let info = file.len();
    for y in 0..h {
        file.extend_from_slice(&bits[y * row_bytes..y * row_bytes + row]);
        file.resize(info + (y + 1) * stride, 0);
    }
    let len = file.len() - info;
    // An indexed bitmap has no colour table: its pixels are indices into the selected palette.
    let at = DibAt { bmi: (0, info as u32), bits: (info as u32, len as u32), usage: if indexed { 2 } else { 0 } };
    // A monochrome pattern keeps its bits packed: its two colours are the text and background colours as drawn.
    let d = if pattern && bpp == 1 { dib_packed(p, Rec(&file), at, false, None)? } else { dib(p, Rec(&file), at, false, None)? };
    Some((d, bpp == 1))
}

/// The EMF a file carries in its escape records (`META_ESCAPE_ENHANCED_METAFILE`), reassembled from their chunks
/// within [`MAX_EMBEDDED`] bytes; `None` when it carries no complete one.
fn embedded_emf(d: &[u8], first: usize) -> Option<Vec<u8>> {
    let (mut at, mut n) = (first, 0u32);
    let mut data = Vec::new();
    while n < MAX_RECORDS {
        let (r, size) = next(d, at)?;
        at += size;
        n += 1;
        match r.u16(4)? {
            0 => return None,
            0x0626 if r.u16(6) == Some(0x7FFF) && r.u32(10) == Some(0x4346_4D57) && r.u32(14) == Some(1) => {}
            _ => continue,
        }
        let (Some(current), Some(remaining)) = (r.u32(32), r.u32(36)) else { continue };
        let Some(chunk) = r.slice(44, current as usize, 1) else { continue };
        if data.len().saturating_add(chunk.len()) > MAX_EMBEDDED || remaining as usize > MAX_EMBEDDED {
            data = Vec::new();
            continue;
        }
        data.extend_from_slice(chunk);
        if remaining == 0 {
            if emf::Header::read(&data).is_some() {
                return Some(data);
            }
            data = Vec::new();
        }
    }
    None
}

/// A region object's rects: each scan's top and bottom, with a left and right for each of its runs.
fn scans(r: &Rec<'_>) -> Option<Vec<kurbo::Rect>> {
    let count = usize::from(r.u16(16)?);
    let (mut at, mut rects) = (28usize, Vec::new());
    for _ in 0..count {
        let n = usize::from(r.u16(at)?);
        let (top, bottom) = (f64::from(r.u16(at + 2)?), f64::from(r.u16(at + 4)?));
        for run in r.slice(at + 6, n / 2, 4)?.as_chunks::<4>().0 {
            let x = |k: usize| f64::from(u16::from_le_bytes([run[k], run[k + 1]]));
            rects.push(kurbo::Rect::new(x(0), top, x(2), bottom));
        }
        at += 6 + n * 2 + 2;
    }
    Some(rects)
}

/// A line of text a record draws: its reference point, character count and where the string, the options and the
/// spacing are in the record.
#[derive(Default)]
struct Line {
    at: Point,
    n: usize,
    string: usize,
    rect: Option<kurbo::Rect>,
    options: u16,
    dx: Option<usize>,
}

/// The line `l` of `r`'s string (in the font's character set) drawn, each character advanced as the spacing says.
fn text<S: Sink>(p: &mut Player<'_, S>, t: &Objects, r: &Rec<'_>, l: Line) -> Option<()> {
    let mut chars = std::mem::take(&mut p.chars);
    chars.clear();
    if !narrow(&mut chars, r.slice(l.string, l.n, 1)?, p.dc.font.charset) {
        p.chars = chars;
        return Some(());
    }
    let mut dx = std::mem::take(&mut p.advances_in);
    dx.clear();
    if let Some(s) = l.dx.and_then(|o| r.slice(o, l.n, 2)) {
        dx.extend(s.as_chunks::<2>().0.iter().map(|c| f64::from(i16::from_le_bytes(*c))));
    }
    let out = TextOut {
        at: l.at,
        chars: &chars,
        glyph_ids: false,
        advances: (!dx.is_empty()).then_some(&dx[..]),
        rises: None,
        extra: t.extra(),
        rect: l.rect.filter(|r| r.width() != 0.0 && r.height() != 0.0),
        opaque: l.options & 2 != 0,
        clipped: l.options & 4 != 0,
    };
    p.text(out);
    p.chars = chars;
    p.advances_in = dx;
    Some(())
}
