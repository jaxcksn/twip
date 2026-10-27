//! What an image file says about itself: its pixel size and resolution.

/// Pixel size (orientation applied) and resolution of an image.
pub fn image_info(data: &[u8]) -> Option<(f64, f64, f64, f64)> {
    let codec = skia_safe::Codec::from_data(skia_safe::Data::new_copy(data))?;
    let d = codec.dimensions();
    let (dx, dy) = dpi(data);
    let swap = !matches!(
        codec.origin(),
        skia_safe::EncodedOrigin::TopLeft
            | skia_safe::EncodedOrigin::TopRight
            | skia_safe::EncodedOrigin::BottomRight
            | skia_safe::EncodedOrigin::BottomLeft
    );
    Some(if swap { (d.height as f64, d.width as f64, dy, dx) } else { (d.width as f64, d.height as f64, dx, dy) })
}

/// PNG `pHYs` or JPEG JFIF density, else 96 dpi.
fn dpi(d: &[u8]) -> (f64, f64) {
    let be = |p: usize| u32::from_be_bytes([d[p], d[p + 1], d[p + 2], d[p + 3]]);
    if d.len() > 16 && d[0] == 0x89 && d[1] == b'P' {
        let mut p = 8;
        while p + 12 <= d.len() {
            let len = be(p) as usize;
            if p + 12 + len > d.len() {
                break;
            }
            if &d[p + 4..p + 8] == b"pHYs" && len >= 9 && d[p + 16] == 1 {
                return (be(p + 8) as f64 * 0.0254, be(p + 12) as f64 * 0.0254);
            }
            if &d[p + 4..p + 6] == b"ID" {
                break;
            }
            p += 12 + len;
        }
    }
    if d.len() > 20 && d[..4] == [0xFF, 0xD8, 0xFF, 0xE0] && &d[6..10] == b"JFIF" {
        let (units, x, y) = (d[13], (d[14] as u32) << 8 | d[15] as u32, (d[16] as u32) << 8 | d[17] as u32);
        let k = match units {
            1 => 1.0,
            2 => 2.54,
            _ => 0.0,
        };
        if k > 0.0 && x > 0 && y > 0 {
            return (x as f64 * k, y as f64 * k);
        }
    }
    (96.0, 96.0)
}

/// MIME type and extension from the file signature (PNG, JPEG, GIF, BMP, TIFF).
pub fn image_type(d: &[u8]) -> Option<(&'static str, &'static str)> {
    if d.len() < 4 {
        return None;
    }
    Some(match d {
        [0x89, b'P', b'N', b'G', ..] => ("image/png", "png"),
        [0xFF, 0xD8, ..] => ("image/jpeg", "jpeg"),
        [b'G', b'I', b'F', ..] => ("image/gif", "gif"),
        [b'B', b'M', ..] => ("image/bmp", "bmp"),
        [b'I', b'I', 42, _, ..] | [b'M', b'M', _, 42, ..] => ("image/tiff", "tiff"),
        _ => return None,
    })
}
