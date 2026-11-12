//! An atlas: square pages of texels (8-bit coverage, or colour glyphs' RGBA), packed with a shelf allocator (rows of
//! slots as tall as the first slot that opened them), kept on the CPU and uploaded to the pages of a texture array.
//! Glyph masks, path masks and clip-applied coverage live in one; colour glyphs in another. The shader reads a texel
//! per pixel, unfiltered, so slots need no padding.

/// Where a slot is: its page, and its top left texel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Slot {
    pub page: u32,
    pub x: u32,
    pub y: u32,
}

#[derive(Debug)]
struct Shelf {
    y: u32,
    height: u32,
    /// The next free column.
    x: u32,
}

#[derive(Debug)]
struct Page {
    shelves: Vec<Shelf>,
    /// The first row no shelf has taken.
    top: u32,
    pixels: Vec<u8>,
    /// The rows written since the last upload: `top..bottom`, empty when `top >= bottom`.
    dirty: (u32, u32),
}

/// The atlas's pages, `size` texels square, of `texel` bytes each.
#[derive(Debug)]
pub(super) struct Atlas {
    size: u32,
    texel: usize,
    pages: Vec<Page>,
}

impl Atlas {
    pub fn new(size: u32, texel: usize) -> Atlas {
        Atlas { size, texel, pages: Vec::new() }
    }

    pub fn pages(&self) -> usize {
        self.pages.len()
    }

    /// Empties every page, keeping their memory.
    pub fn clear(&mut self) {
        for p in &mut self.pages {
            p.shelves.clear();
            p.top = 0;
            p.dirty = (u32::MAX, 0);
        }
    }

    /// A `w × h` slot, on the first page with room (a new page when none has); `None` when it is larger than a page.
    pub fn alloc(&mut self, w: u32, h: u32) -> Option<Slot> {
        if w == 0 || h == 0 || w > self.size || h > self.size {
            return None;
        }
        for (i, page) in self.pages.iter_mut().enumerate() {
            if let Some((x, y)) = place(page, self.size, w, h) {
                return Some(Slot { page: i as u32, x, y });
            }
        }
        let n = self.size as usize;
        self.pages.push(Page { shelves: Vec::new(), top: 0, pixels: vec![0; n * n * self.texel], dirty: (u32::MAX, 0) });
        let page = self.pages.last_mut()?;
        let (x, y) = place(page, self.size, w, h)?;
        Some(Slot { page: (self.pages.len() - 1) as u32, x, y })
    }

    /// Row `row` of slot `at`, `w` texels wide, to write into.
    pub fn row(&mut self, at: Slot, row: u32, w: u32) -> &mut [u8] {
        let (size, texel) = (self.size as usize, self.texel);
        let page = &mut self.pages[at.page as usize];
        let y = at.y + row;
        page.dirty = (page.dirty.0.min(y), page.dirty.1.max(y + 1));
        let start = (y as usize * size + at.x as usize) * texel;
        &mut page.pixels[start..start + w as usize * texel]
    }

    /// Each page's rows written since the last call, as (page, first row, rows, their texels), the record cleared.
    pub fn take_dirty(&mut self) -> impl Iterator<Item = (u32, u32, u32, &[u8])> {
        let row = self.size as usize * self.texel;
        self.pages.iter_mut().enumerate().filter_map(move |(i, p)| {
            let (top, bottom) = std::mem::replace(&mut p.dirty, (u32::MAX, 0));
            (top < bottom).then(|| (i as u32, top, bottom - top, &p.pixels[top as usize * row..bottom as usize * row]))
        })
    }
}

/// A place for `w × h` on `page`: on the first shelf it fits (the shortest that's tall enough), else on a new shelf.
fn place(page: &mut Page, size: u32, w: u32, h: u32) -> Option<(u32, u32)> {
    let fits = |s: &Shelf| s.height >= h && size - s.x >= w;
    if let Some(s) = page.shelves.iter_mut().filter(|s| fits(s)).min_by_key(|s| s.height) {
        let x = s.x;
        s.x += w;
        return Some((x, s.y));
    }
    if size - page.top < h {
        return None;
    }
    let y = page.top;
    page.top += h;
    page.shelves.push(Shelf { y, height: h, x: w });
    Some((0, y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_pack_on_shelves_without_overlapping() {
        let mut a = Atlas::new(64, 1);
        let slots: Vec<(Slot, u32, u32)> = (0..40).map(|i| (3 + i % 7, 2 + i % 5)).filter_map(|(w, h)| a.alloc(w, h).map(|s| (s, w, h))).collect();
        assert_eq!(slots.len(), 40, "all fit in one or two pages");
        for (i, (s, w, h)) in slots.iter().enumerate() {
            assert!(s.x + w <= 64 && s.y + h <= 64);
            for (t, tw, th) in &slots[i + 1..] {
                let apart = s.page != t.page || s.x + w <= t.x || t.x + tw <= s.x || s.y + h <= t.y || t.y + th <= s.y;
                assert!(apart, "{s:?} {w}×{h} overlaps {t:?} {tw}×{th}");
            }
        }
        assert_eq!(a.alloc(65, 1), None, "larger than a page");
        // A full page opens the next.
        let mut b = Atlas::new(16, 1);
        assert_eq!(b.alloc(16, 16).map(|s| s.page), Some(0));
        assert_eq!(b.alloc(1, 1).map(|s| s.page), Some(1));
    }

    #[test]
    fn written_rows_are_uploaded_once() {
        let mut a = Atlas::new(8, 1);
        let s = a.alloc(2, 3).expect("a slot");
        a.row(s, 1, 2).copy_from_slice(&[7, 9]);
        let dirty: Vec<(u32, u32, u32, usize)> = a.take_dirty().map(|(p, y, n, px)| (p, y, n, px.len())).collect();
        assert_eq!(dirty, [(0, s.y + 1, 1, 8)]);
        assert_eq!(a.take_dirty().count(), 0);
        // Four bytes a texel: rows and uploads are four times as long.
        let mut c = Atlas::new(8, 4);
        let s = c.alloc(2, 3).expect("a slot");
        assert_eq!(c.row(s, 2, 2).len(), 8);
        let dirty: Vec<(u32, u32, usize)> = c.take_dirty().map(|(_, y, n, px)| (y, n, px.len())).collect();
        assert_eq!(dirty, [(s.y + 2, 1, 32)]);
    }
}
