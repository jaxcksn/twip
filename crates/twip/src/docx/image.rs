//! What an image file says about itself: its pixel size and resolution.

/// Pixel size (orientation applied) and resolution of an image.
pub fn image_info(data: &[u8]) -> Option<(f64, f64, f64, f64)> {
    // The probe's size already has the EXIF orientation applied: a quarter turn swaps it.
    let info = twip_graphics::image::probe(data)?;
    let (dx, dy) = dpi(data);
    let (w, h) = (f64::from(info.width), f64::from(info.height));
    Some(if info.orientation >= 5 { (w, h, dy, dx) } else { (w, h, dx, dy) })
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

/// Whether the bytes are an SVG document: text whose first element is `svg`.
pub fn is_svg(d: &[u8]) -> bool {
    let head = &d[..d.len().min(4096)];
    let text = match std::str::from_utf8(head) {
        Ok(t) => t,
        Err(e) => std::str::from_utf8(&head[..e.valid_up_to()]).unwrap_or(""),
    };
    let mut rest = text.trim_start_matches('\u{feff}').trim_start();
    // Past the prolog: the XML declaration, comments, processing instructions and a doctype.
    while let Some(r) = rest.strip_prefix("<?").or_else(|| rest.strip_prefix("<!")) {
        let end = if r.starts_with("--") { r.find("-->").map(|k| k + 3) } else { r.find('>').map(|k| k + 1) };
        let Some(end) = end else { return false };
        rest = r[end..].trim_start();
    }
    rest.strip_prefix("<svg").is_some_and(|r| r.starts_with(|c: char| c.is_whitespace() || c == '>' || c == '/'))
}

/// MIME type and extension from the file signature (PNG, JPEG, GIF, BMP, TIFF, SVG).
pub fn image_type(d: &[u8]) -> Option<(&'static str, &'static str)> {
    if d.len() < 4 {
        return None;
    }
    if is_svg(d) {
        return Some(("image/svg+xml", "svg"));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_exif_quarter_turn_swaps_the_size() {
        // Stored 48 × 32 with EXIF orientation 6: shown 32 wide and 48 tall.
        let jpeg = include_bytes!("../../../twip-graphics/tests/images/rotated-6.jpg");
        assert_eq!(image_info(jpeg), Some((32.0, 48.0, 96.0, 96.0)));
        assert_eq!(image_info(b"\x89PNG\r\n\x1a\n not a png"), None);
    }
}
