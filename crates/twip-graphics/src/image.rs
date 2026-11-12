//! Images: what a file says about itself (format, pixel size, EXIF orientation), decoding (PNG, JPEG, GIF's first frame,
//! BMP, ICO, WBMP) into premultiplied RGBA8 with the orientation applied, the mip chain image draws sample from, and the
//! registry pictures name images by.
//!
//! Twip's document reader uses [`probe`] too, in place of `SkCodec`. Decoding is lazy: the registry keeps the encoded
//! bytes, and each renderer decodes an image the first time it draws it, into its own cache.
//!
//! Decoders: `zune-png`, `zune-jpeg`, `zune-bmp` and `gif`; ICO (largest entry, PNG or BMP DIB with its AND mask) and
//! WBMP are read here. EXIF orientation is read from a JPEG's APP1 segment (the TIFF tag 0x0112, as Skia's `SkExif`).
//! An image over 2^28 pixels, or any decoder error, gives no image rather than a panic or unbounded memory.

use std::fmt;
use std::sync::Arc;

use zune_core::bytestream::ZCursor;
use zune_core::colorspace::ColorSpace;
use zune_core::options::DecoderOptions;

use crate::fx::FastMap;

/// An image in the registry. Pictures name images by id, never by pointer, so they own nothing heavy and can be sent
/// between threads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ImageId(pub(crate) u32);

/// Encoded image bytes, shared with the caller.
pub type ImageData = Arc<dyn AsRef<[u8]> + Send + Sync>;

/// The most pixels an image may decode to.
const MAX_PIXELS: u64 = 1 << 28;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Gif,
    Bmp,
    Ico,
    Wbmp,
}

/// What an image file says about itself: its format and its pixel size with the EXIF orientation applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ImageInfo {
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
    /// EXIF orientation, 1 (as stored) to 8.
    pub orientation: u8,
}

/// Bytes that aren't an image of a supported format, or that don't decode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImageError(pub String);

impl fmt::Display for ImageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ImageError {}

fn error(what: impl fmt::Display) -> ImageError {
    ImageError(what.to_string())
}

/// Premultiplied RGBA8 pixels, `width × height`, rows packed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Pixmap {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl Pixmap {
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let at = (y * self.width + x) as usize * 4;
        [self.data[at], self.data[at + 1], self.data[at + 2], self.data[at + 3]]
    }
}

/// The format a file's first bytes announce.
fn format(d: &[u8]) -> Option<ImageFormat> {
    Some(match d {
        [0x89, b'P', b'N', b'G', ..] => ImageFormat::Png,
        [0xFF, 0xD8, 0xFF, ..] => ImageFormat::Jpeg,
        [b'G', b'I', b'F', b'8', ..] => ImageFormat::Gif,
        [b'B', b'M', ..] => ImageFormat::Bmp,
        [0, 0, 1, 0, ..] => ImageFormat::Ico,
        // WBMP type 0: no magic, just a zero type and a zero header byte, then the size.
        [0, 0, ..] => ImageFormat::Wbmp,
        _ => return None,
    })
}

/// What `bytes` say about themselves, reading headers only; `None` when they aren't a supported image.
pub fn probe(bytes: &[u8]) -> Option<ImageInfo> {
    let format = format(bytes)?;
    let (w, h, orientation) = match format {
        ImageFormat::Png => {
            let mut d = zune_png::PngDecoder::new(ZCursor::new(bytes));
            d.decode_headers().ok()?;
            let (w, h) = d.dimensions()?;
            (w as u32, h as u32, 1)
        }
        ImageFormat::Jpeg => {
            let mut d = zune_jpeg::JpegDecoder::new(ZCursor::new(bytes));
            d.decode_headers().ok()?;
            let info = d.info()?;
            (info.width as u32, info.height as u32, d.exif().map_or(1, |e| exif_orientation(e)))
        }
        ImageFormat::Gif => {
            let d = gif::DecodeOptions::new().read_info(std::io::Cursor::new(bytes)).ok()?;
            (d.width() as u32, d.height() as u32, 1)
        }
        ImageFormat::Bmp => {
            let mut d = zune_bmp::BmpDecoder::new(ZCursor::new(bytes));
            d.decode_headers().ok()?;
            let (w, h) = d.dimensions()?;
            (w as u32, h as u32, 1)
        }
        ImageFormat::Ico => {
            let e = ico_entry(bytes)?;
            (e.width, e.height, 1)
        }
        ImageFormat::Wbmp => {
            let (w, h, _) = wbmp_header(bytes)?;
            (w, h, 1)
        }
    };
    if w == 0 || h == 0 {
        return None;
    }
    let (width, height) = if orientation >= 5 { (h, w) } else { (w, h) };
    Some(ImageInfo { format, width, height, orientation })
}

/// Decodes `bytes` into premultiplied RGBA8, orientation applied.
pub(crate) fn decode(bytes: &[u8]) -> Result<Pixmap, ImageError> {
    let info = probe(bytes).ok_or_else(|| error("not a supported image"))?;
    if info.width as u64 * info.height as u64 > MAX_PIXELS {
        return Err(error(format_args!("{}×{} pixels is more than an image may have", info.width, info.height)));
    }
    let options = DecoderOptions::default().set_max_width(1 << 16).set_max_height(1 << 16);
    let straight = match info.format {
        ImageFormat::Png => png(bytes, options)?,
        ImageFormat::Jpeg => {
            let options = options.jpeg_set_out_colorspace(ColorSpace::RGBA);
            let mut d = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(bytes), options);
            let data = d.decode().map_err(|e| error(format_args!("{e:?}")))?;
            let i = d.info().ok_or_else(|| error("no JPEG header"))?;
            to_rgba(i.width as u32, i.height as u32, d.output_colorspace().unwrap_or(ColorSpace::RGBA), data)?
        }
        ImageFormat::Gif => gif_first_frame(bytes)?,
        ImageFormat::Bmp => bmp(bytes, options)?,
        ImageFormat::Ico => ico(bytes, options)?,
        ImageFormat::Wbmp => wbmp(bytes)?,
    };
    let mut p = orient(straight, info.orientation);
    premultiply(&mut p.data);
    Ok(p)
}

fn png(bytes: &[u8], options: DecoderOptions) -> Result<Pixmap, ImageError> {
    let options = options.png_set_strip_to_8bit(true).png_set_add_alpha_channel(true);
    let mut d = zune_png::PngDecoder::new_with_options(ZCursor::new(bytes), options);
    let data = d.decode_raw().map_err(|e| error(format_args!("{e:?}")))?;
    let (w, h) = d.dimensions().ok_or_else(|| error("no PNG header"))?;
    to_rgba(w as u32, h as u32, d.colorspace().unwrap_or(ColorSpace::RGBA), data)
}

fn bmp(bytes: &[u8], options: DecoderOptions) -> Result<Pixmap, ImageError> {
    let mut d = zune_bmp::BmpDecoder::new_with_options(ZCursor::new(bytes), options);
    let data = d.decode().map_err(|e| error(format_args!("{e:?}")))?;
    let (w, h) = d.dimensions().ok_or_else(|| error("no BMP header"))?;
    to_rgba(w as u32, h as u32, d.colorspace().unwrap_or(ColorSpace::RGB), data)
}

/// Straight (unpremultiplied) RGBA8 from a decoder's 8-bit output in `space`.
fn to_rgba(width: u32, height: u32, space: ColorSpace, data: Vec<u8>) -> Result<Pixmap, ImageError> {
    let n = width as usize * height as usize;
    let data = match space {
        ColorSpace::RGBA => data,
        ColorSpace::RGB => data.as_chunks::<3>().0.iter().flat_map(|&[r, g, b]| [r, g, b, 255]).collect(),
        ColorSpace::BGRA => data.as_chunks::<4>().0.iter().flat_map(|&[b, g, r, a]| [r, g, b, a]).collect(),
        ColorSpace::BGR => data.as_chunks::<3>().0.iter().flat_map(|&[b, g, r]| [r, g, b, 255]).collect(),
        ColorSpace::Luma => data.iter().flat_map(|&v| [v, v, v, 255]).collect(),
        ColorSpace::LumaA => data.as_chunks::<2>().0.iter().flat_map(|&[l, a]| [l, l, l, a]).collect(),
        other => return Err(error(format_args!("decoder output {other:?} isn't handled"))),
    };
    if data.len() != n * 4 {
        return Err(error("the decoder gave the wrong number of pixels"));
    }
    Ok(Pixmap { width, height, data })
}

/// GIF's first frame, placed on its logical screen (transparent elsewhere), as Skia draws an animated GIF still.
fn gif_first_frame(bytes: &[u8]) -> Result<Pixmap, ImageError> {
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    let mut d = options.read_info(std::io::Cursor::new(bytes)).map_err(error)?;
    let (w, h) = (d.width() as u32, d.height() as u32);
    let frame = d.read_next_frame().map_err(error)?.ok_or_else(|| error("a GIF without frames"))?;
    let mut out = vec![0u8; w as usize * h as usize * 4];
    for row in 0..frame.height as u32 {
        let y = frame.top as u32 + row;
        if y >= h {
            break;
        }
        for col in 0..frame.width as u32 {
            let x = frame.left as u32 + col;
            let src = (row * frame.width as u32 + col) as usize * 4;
            if x < w && src + 4 <= frame.buffer.len() {
                let dst = (y * w + x) as usize * 4;
                out[dst..dst + 4].copy_from_slice(&frame.buffer[src..src + 4]);
            }
        }
    }
    Ok(Pixmap { width: w, height: h, data: out })
}

struct IcoEntry {
    width: u32,
    height: u32,
    bpp: u16,
    offset: usize,
    size: usize,
}

/// The ICO directory's largest entry (most pixels, then most bits per pixel).
fn ico_entry(d: &[u8]) -> Option<IcoEntry> {
    let u16_at = |o: usize| d.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]));
    let u32_at = |o: usize| d.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let count = u16_at(4)? as usize;
    (0..count)
        .filter_map(|i| {
            let e = 6 + 16 * i;
            let side = |b: u8| if b == 0 { 256 } else { b as u32 };
            Some(IcoEntry {
                width: side(*d.get(e)?),
                height: side(*d.get(e + 1)?),
                bpp: u16_at(e + 6)?,
                size: u32_at(e + 8)? as usize,
                offset: u32_at(e + 12)? as usize,
            })
        })
        .max_by_key(|e| (e.width as u64 * e.height as u64, e.bpp))
}

/// An ICO's largest entry: a PNG, or a BMP DIB (its height doubled for the AND mask that follows the colours).
fn ico(bytes: &[u8], options: DecoderOptions) -> Result<Pixmap, ImageError> {
    let e = ico_entry(bytes).ok_or_else(|| error("an ICO without entries"))?;
    let data = bytes.get(e.offset..e.offset.saturating_add(e.size)).ok_or_else(|| error("an ICO entry outside the file"))?;
    if data.starts_with(&[0x89, b'P', b'N', b'G']) {
        return png(data, options);
    }
    let u32_at = |o: usize| data.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let header = u32_at(0).ok_or_else(|| error("a truncated ICO bitmap"))? as usize;
    let bpp = data.get(14..16).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]));
    let colours = match u32_at(32).unwrap_or(0) {
        0 if bpp <= 8 => 1usize << bpp,
        n => n as usize,
    };
    let pixels_at = 14 + header + colours * 4;
    // A BMP file: a file header, the DIB header with the height halved (the colour rows only), the palette and rows.
    let mut file = Vec::with_capacity(14 + data.len());
    file.extend_from_slice(b"BM");
    file.extend_from_slice(&((14 + data.len()) as u32).to_le_bytes());
    file.extend_from_slice(&[0; 4]);
    file.extend_from_slice(&(pixels_at as u32).to_le_bytes());
    file.extend_from_slice(data);
    let h = e.height as i32;
    file[14 + 8..14 + 12].copy_from_slice(&h.to_le_bytes());
    let mut p = bmp(&file, options)?;
    // The AND mask: 1 bit per pixel, rows bottom up, each padded to 4 bytes; a set bit is transparent. A 32-bit
    // bitmap carries its own alpha, unless every alpha is zero (then the mask applies).
    let has_alpha = bpp == 32 && p.data.as_chunks::<4>().0.iter().any(|q| q[3] != 0);
    if !has_alpha {
        let row_bytes = (p.width as usize).div_ceil(32) * 4;
        let colour_bytes = (p.width as usize * bpp as usize).div_ceil(32) * 4 * p.height as usize;
        let mask_at = header + colours * 4 + colour_bytes;
        for y in 0..p.height as usize {
            let src_row = p.height as usize - 1 - y;
            for x in 0..p.width as usize {
                let bit = data.get(mask_at + src_row * row_bytes + x / 8).map_or(0, |b| (b >> (7 - x % 8)) & 1);
                p.data[(y * p.width as usize + x) * 4 + 3] = if bit == 1 { 0 } else { 255 };
            }
        }
    }
    Ok(p)
}

/// WBMP type 0: width and height as multi-byte integers, then 1-bit rows (padded to bytes), 1 white and 0 black.
fn wbmp_header(d: &[u8]) -> Option<(u32, u32, usize)> {
    if d.first() != Some(&0) || d.get(1) != Some(&0) {
        return None;
    }
    let mut at = 2;
    let mut read = || {
        let mut v: u32 = 0;
        for _ in 0..4 {
            let b = *d.get(at)?;
            at += 1;
            v = (v << 7) | (b & 0x7F) as u32;
            if b & 0x80 == 0 {
                return Some(v);
            }
        }
        None
    };
    let (w, h) = (read()?, read()?);
    let row = (w as usize).div_ceil(8);
    (d.len() >= at + row * h as usize).then_some((w, h, at))
}

fn wbmp(d: &[u8]) -> Result<Pixmap, ImageError> {
    let (w, h, at) = wbmp_header(d).ok_or_else(|| error("a truncated WBMP"))?;
    let row = (w as usize).div_ceil(8);
    let mut data = Vec::with_capacity(w as usize * h as usize * 4);
    for y in 0..h as usize {
        for x in 0..w as usize {
            let v = if (d[at + y * row + x / 8] >> (7 - x % 8)) & 1 == 1 { 255 } else { 0 };
            data.extend_from_slice(&[v, v, v, 255]);
        }
    }
    Ok(Pixmap { width: w, height: h, data })
}

/// The EXIF orientation (1 to 8) in a JPEG's APP1 payload (with or without its `Exif\0\0` prefix), 1 when absent.
fn exif_orientation(exif: &[u8]) -> u8 {
    let t = exif.strip_prefix(b"Exif\0\0").unwrap_or(exif);
    let little = match t.get(0..2) {
        Some(b"II") => true,
        Some(b"MM") => false,
        _ => return 1,
    };
    let u16_at = |o: usize| t.get(o..o + 2).map(|b| if little { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) });
    let u32_at = |o: usize| {
        t.get(o..o + 4).map(|b| if little { u32::from_le_bytes([b[0], b[1], b[2], b[3]]) } else { u32::from_be_bytes([b[0], b[1], b[2], b[3]]) })
    };
    let Some(ifd) = u32_at(4).map(|v| v as usize) else { return 1 };
    let count = u16_at(ifd).unwrap_or(0) as usize;
    (0..count)
        .find_map(|i| {
            let e = ifd + 2 + 12 * i;
            (u16_at(e)? == 0x0112).then(|| u16_at(e + 8)).flatten()
        })
        .filter(|o| (1..=8).contains(o))
        .map_or(1, |o| o as u8)
}

/// `p` turned to EXIF orientation `o`'s upright view.
fn orient(p: Pixmap, o: u8) -> Pixmap {
    if o <= 1 || o > 8 {
        return p;
    }
    let (w, h) = (p.width, p.height);
    let (ow, oh) = if o >= 5 { (h, w) } else { (w, h) };
    let mut data = vec![0u8; p.data.len()];
    for y in 0..oh {
        for x in 0..ow {
            let (sx, sy) = match o {
                2 => (w - 1 - x, y),
                3 => (w - 1 - x, h - 1 - y),
                4 => (x, h - 1 - y),
                5 => (y, x),
                6 => (y, h - 1 - x),
                7 => (w - 1 - y, h - 1 - x),
                _ => (w - 1 - y, x),
            };
            let (src, dst) = (((sy * w + sx) * 4) as usize, ((y * ow + x) * 4) as usize);
            data[dst..dst + 4].copy_from_slice(&p.data[src..src + 4]);
        }
    }
    Pixmap { width: ow, height: oh, data }
}

fn premultiply(data: &mut [u8]) {
    for p in data.as_chunks_mut::<4>().0 {
        let a = p[3];
        if a != 255 {
            for c in &mut p[..3] {
                *c = crate::paint::mul_div255(*c, a);
            }
        }
    }
}

/// An image's mip chain, as Skia's `SkMipmap` builds it: level 0 the image, each next level half the size (rounded
/// down, at least 1, the sizes a GPU texture's levels have), on premultiplied pixels, down to 1×1. Along an even side
/// a pixel averages the two above it; along an odd one, three, weighted 1, 2, 1, so the last row or column counts too.
#[derive(Clone, Debug)]
pub(crate) struct Mips {
    pub levels: Vec<Pixmap>,
}

/// The taps along one side, from `n` pixels to half as many: each output's first input and weights.
fn taps(n: u32, i: u32) -> (u32, &'static [u32]) {
    match n {
        1 => (0, &[1]),
        n if n % 2 == 0 => (2 * i, &[1, 1]),
        _ => (2 * i, &[1, 2, 1]),
    }
}

impl Mips {
    pub fn new(base: Pixmap) -> Mips {
        let mut levels = vec![base];
        while let Some(last) = levels.last().filter(|l| l.width > 1 || l.height > 1) {
            let (w, h) = ((last.width / 2).max(1), (last.height / 2).max(1));
            let mut data = Vec::with_capacity((w * h * 4) as usize);
            for y in 0..h {
                let (y0, wy) = taps(last.height, y);
                for x in 0..w {
                    let (x0, wx) = taps(last.width, x);
                    let total: u32 = wx.iter().sum::<u32>() * wy.iter().sum::<u32>();
                    let mut sum = [0u32; 4];
                    for (j, &ky) in wy.iter().enumerate() {
                        for (i, &kx) in wx.iter().enumerate() {
                            let p = last.pixel(x0 + i as u32, y0 + j as u32);
                            (0..4).for_each(|c| sum[c] += p[c] as u32 * kx * ky);
                        }
                    }
                    data.extend(sum.map(|s| ((s + total / 2) / total) as u8));
                }
            }
            levels.push(Pixmap { width: w, height: h, data });
        }
        Mips { levels }
    }

    pub fn bytes(&self) -> usize {
        self.levels.iter().map(|l| l.data.len()).sum()
    }
}

enum Entry {
    Encoded(ImageData),
    /// Straight RGBA8 pixels the caller made (Twip's 8 × 8 pattern tiles).
    Pixels(Arc<Pixmap>),
}

/// The images pictures draw.
#[derive(Default)]
pub struct ImageRegistry {
    entries: Vec<(Entry, ImageInfo)>,
    /// Encoded images by their bytes' address, so registering one twice gives the same id. The entry holds the
    /// bytes, so the address isn't reused while the key is in the map.
    known: FastMap<usize, ImageId>,
}

impl fmt::Debug for ImageRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImageRegistry").field("images", &self.entries.len()).finish()
    }
}

impl ImageRegistry {
    /// Registers an encoded image, reading its header. Fails for bytes that aren't a supported image.
    pub fn add(&mut self, data: ImageData) -> Result<ImageId, ImageError> {
        let address = Arc::as_ptr(&data) as *const () as usize;
        if let Some(&id) = self.known.get(&address) {
            return Ok(id);
        }
        let info = probe((*data).as_ref()).ok_or_else(|| error("not a supported image"))?;
        let id = ImageId(self.entries.len() as u32);
        self.entries.push((Entry::Encoded(data), info));
        self.known.insert(address, id);
        Ok(id)
    }

    /// Registers `width × height` straight (unpremultiplied) RGBA8 pixels.
    pub fn add_pixels(&mut self, width: u32, height: u32, mut rgba: Vec<u8>) -> Result<ImageId, ImageError> {
        if width == 0 || height == 0 || rgba.len() as u64 != width as u64 * height as u64 * 4 {
            return Err(error(format_args!("{} bytes aren't {width}×{height} RGBA pixels", rgba.len())));
        }
        premultiply(&mut rgba);
        let info = ImageInfo { format: ImageFormat::Png, width, height, orientation: 1 };
        let id = ImageId(self.entries.len() as u32);
        self.entries.push((Entry::Pixels(Arc::new(Pixmap { width, height, data: rgba })), info));
        Ok(id)
    }

    pub fn info(&self, id: ImageId) -> Option<ImageInfo> {
        self.entries.get(id.0 as usize).map(|e| e.1)
    }

    /// The image's pixels: decoded now for an encoded image.
    pub(crate) fn decode(&self, id: ImageId) -> Result<Pixmap, ImageError> {
        match self.entries.get(id.0 as usize) {
            Some((Entry::Encoded(data), _)) => decode((**data).as_ref()),
            Some((Entry::Pixels(p), _)) => Ok((**p).clone()),
            None => Err(error("no such image")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_bytes(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header().expect("header").write_image_data(rgba).expect("data");
        out
    }

    #[test]
    fn png_decodes_premultiplied() {
        let bytes = png_bytes(2, 1, &[255, 0, 0, 255, 255, 255, 255, 128]);
        assert_eq!(probe(&bytes), Some(ImageInfo { format: ImageFormat::Png, width: 2, height: 1, orientation: 1 }));
        assert_eq!(decode(&bytes).expect("decodes").data, [255, 0, 0, 255, 128, 128, 128, 128]);
    }

    #[test]
    fn wbmp_reads_its_bits() {
        // 10 × 2: row 0 starts white, row 1 starts black.
        let bytes = [0, 0, 10, 2, 0b1000_0000, 0b0100_0000, 0b0111_1111, 0b1100_0000];
        let p = decode(&bytes).expect("decodes");
        assert_eq!((p.width, p.height), (10, 2));
        assert_eq!(p.pixel(0, 0), [255; 4]);
        assert_eq!(p.pixel(1, 0), [0, 0, 0, 255]);
        assert_eq!(p.pixel(9, 0), [255; 4]);
        assert_eq!(p.pixel(0, 1), [0, 0, 0, 255]);
    }

    #[test]
    fn ico_takes_its_largest_png_entry() {
        let small = png_bytes(1, 1, &[0, 0, 255, 255]);
        let large = png_bytes(2, 2, &[0, 255, 0, 255].repeat(4));
        let mut ico = vec![0, 0, 1, 0, 2, 0];
        let first = 6 + 32;
        for (side, data, at) in [(1u8, &small, first), (2, &large, first + small.len())] {
            ico.extend_from_slice(&[side, side, 0, 0, 1, 0, 32, 0]);
            ico.extend_from_slice(&(data.len() as u32).to_le_bytes());
            ico.extend_from_slice(&(at as u32).to_le_bytes());
        }
        ico.extend_from_slice(&small);
        ico.extend_from_slice(&large);
        let p = decode(&ico).expect("decodes");
        assert_eq!((p.width, p.height, p.pixel(1, 1)), (2, 2, [0, 255, 0, 255]));
    }

    #[test]
    fn orientations_turn_the_pixels_upright() {
        // A 2 × 1 image, A then B; orientation 6 is a quarter turn clockwise to view.
        let a = [1, 0, 0, 255];
        let b = [2, 0, 0, 255];
        let p = Pixmap { width: 2, height: 1, data: [a, b].concat() };
        let turned = orient(p.clone(), 6);
        assert_eq!((turned.width, turned.height), (1, 2));
        assert_eq!((turned.pixel(0, 0), turned.pixel(0, 1)), (a, b));
        let flipped = orient(p, 2);
        assert_eq!((flipped.pixel(0, 0), flipped.pixel(1, 0)), (b, a));
    }

    #[test]
    fn exif_orientation_reads_both_byte_orders() {
        let tiff = |little: bool| {
            let w16 = |v: u16| if little { v.to_le_bytes() } else { v.to_be_bytes() };
            let w32 = |v: u32| if little { v.to_le_bytes() } else { v.to_be_bytes() };
            let mut t = Vec::new();
            t.extend_from_slice(if little { b"II" } else { b"MM" });
            t.extend_from_slice(&w16(42));
            t.extend_from_slice(&w32(8));
            t.extend_from_slice(&w16(1));
            t.extend_from_slice(&w16(0x0112));
            t.extend_from_slice(&w16(3));
            t.extend_from_slice(&w32(1));
            t.extend_from_slice(&w16(6));
            t.extend_from_slice(&[0, 0]);
            t
        };
        assert_eq!(exif_orientation(&[b"Exif\0\0".as_slice(), &tiff(true)].concat()), 6);
        assert_eq!(exif_orientation(&tiff(false)), 6);
        assert_eq!(exif_orientation(b"junk"), 1);
    }

    #[test]
    fn mips_halve_and_average() {
        // An odd side halves rounded down, its three pixels weighted 1, 2, 1: the last one counts.
        let p = Pixmap { width: 3, height: 1, data: [[0, 0, 0, 0], [200, 200, 200, 200], [100, 100, 100, 100]].concat() };
        let m = Mips::new(p);
        let sizes: Vec<_> = m.levels.iter().map(|l| (l.width, l.height)).collect();
        assert_eq!(sizes, [(3, 1), (1, 1)], "a GPU texture's level sizes");
        assert_eq!(m.levels[1].pixel(0, 0), [125; 4]);
        // An even side averages pairs.
        let p = Pixmap { width: 4, height: 2, data: [[40; 4], [80; 4], [0; 4], [10; 4], [40; 4], [80; 4], [0; 4], [10; 4]].concat() };
        let m = Mips::new(p);
        assert_eq!((m.levels[1].width, m.levels[1].height, m.levels[1].pixel(0, 0), m.levels[1].pixel(1, 0)), (2, 1, [60; 4], [5; 4]));
    }

    #[test]
    fn broken_and_huge_images_fail_without_panicking() {
        for bytes in [&b"\x89PNG\r\n\x1a\n garbage"[..], &[0xFF, 0xD8, 0xFF, 0xE0, 1, 2], b"GIF89a", b"BM", &[0, 0, 1, 0, 9, 0], &[0, 0, 0x8F]] {
            assert!(decode(bytes).is_err(), "{bytes:?}");
        }
        // A WBMP claiming 2^14 × 2^15 pixels with no data.
        assert!(decode(&[0, 0, 0x81, 0x80, 0x00, 0x82, 0x80, 0x00]).is_err());
    }

    #[test]
    fn the_registry_dedupes_and_keeps_pixels() {
        let mut images = ImageRegistry::default();
        let data: ImageData = Arc::new(png_bytes(1, 1, &[9, 9, 9, 255]));
        let a = images.add(data.clone()).expect("an image");
        assert_eq!(images.add(data), Ok(a));
        assert!(images.add(Arc::new(b"nope".to_vec())).is_err());
        let p = images.add_pixels(1, 1, vec![255, 0, 0, 128]).expect("pixels");
        assert_eq!(images.decode(p).expect("pixels").data, [128, 0, 0, 128]);
        assert!(images.add_pixels(2, 2, vec![0; 4]).is_err());
    }
}
