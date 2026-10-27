//! Floating pictures and text boxes: positions (FLOAT-6, FLOAT-9), the free intervals text wraps into (FLOAT-1 to
//! FLOAT-8), and overlap avoidance (FLOAT-12).

use std::rc::Rc;

use crate::docx::model::Float;

use super::ids::ParaId;

const EPS: f64 = 1e-6;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wrap {
    /// square, tight, through (rectangular, FLOAT-8)
    Square,
    TopAndBottom,
    None,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Both,
    Left,
    Right,
    Largest,
}

/// A float placed on a page.
#[derive(Clone, Debug)]
pub struct Placed {
    /// (section, block, piece of the anchor in its paragraph): identifies the float across a page's attempts.
    pub key: (usize, usize, usize),
    /// The anchor paragraph (fragments name it).
    pub anchor: ParaId,
    /// The float's box, page coordinates.
    pub rect: [f64; 4],
    /// The exclusion: the box grown by distT/B/L/R (FLOAT-7).
    pub excl: [f64; 4],
    pub wrap: Wrap,
    pub side: Side,
    pub allow_overlap: bool,
    pub float: Rc<Float>,
}

/// The page boxes a float positions against (FLOAT-6).
pub struct Frames {
    pub page_w: f64,
    pub page_h: f64,
    /// Text area: left/right margins, top/bottom margins.
    pub margin: [f64; 4],
    /// The anchor's column: x, width.
    pub column: (f64, f64),
}

impl Placed {
    pub fn new(key: (usize, usize, usize), anchor: ParaId, f: &Rc<Float>, x: f64, y: f64, h: f64) -> Placed {
        let rect = [x, y, x + f.w, y + h];
        let [dt, db, dl, dr] = f.dist;
        let wrap = match f.wrap.as_str() {
            "square" | "tight" | "through" => Wrap::Square,
            "topAndBottom" => Wrap::TopAndBottom,
            _ => Wrap::None,
        };
        let side = match f.wrap_text.as_str() {
            "left" => Side::Left,
            "right" => Side::Right,
            "largest" => Side::Largest,
            _ => Side::Both,
        };
        Placed {
            key,
            anchor,
            rect,
            excl: [rect[0] - dl, rect[1] - dt, rect[2] + dr, rect[3] + db],
            wrap: if f.behind { Wrap::None } else { wrap },
            side,
            allow_overlap: f.allow_overlap,
            float: f.clone(),
        }
    }

    pub fn same(&self, o: &Placed) -> bool {
        self.key == o.key && self.rect.iter().zip(&o.rect).all(|(a, b)| (a - b).abs() < 1e-3)
    }

    /// FLOAT-1: half-open overlap of a horizontal band with the exclusion.
    pub fn in_band(&self, top: f64, bottom: f64) -> bool {
        self.wrap != Wrap::None && top < self.excl[3] - EPS && self.excl[1] < bottom - EPS
    }

    /// Moves the box, keeping its distances.
    fn moved_to(&mut self, x: f64, y: f64) {
        let (dx, dy) = (x - self.rect[0], y - self.rect[1]);
        for (k, d) in [dx, dy, dx, dy].iter().enumerate() {
            self.rect[k] += d;
            self.excl[k] += d;
        }
    }

    /// Whether two floats must avoid each other (FLOAT-12).
    fn avoids(&self, o: &Placed) -> bool {
        (!self.allow_overlap || !o.allow_overlap) && ((self.wrap == Wrap::None) == (o.wrap == Wrap::None))
    }

    /// Half-open overlap of the bare boxes (FLOAT-12: distances ignored).
    fn overlaps(&self, o: &Placed) -> bool {
        self.rect[0] < o.rect[2] - EPS && o.rect[0] < self.rect[2] - EPS && self.rect[1] < o.rect[3] - EPS && o.rect[1] < self.rect[3] - EPS
    }
}

/// Moves `f` (later in document order) off the floats it may not overlap (FLOAT-12). Nothing fits: the requested
/// position stays and false is returned.
pub fn avoid(f: &mut Placed, placed: &[Placed], page_w: f64, body_bottom: f64) -> bool {
    let original = (f.rect[0], f.rect[1]);
    for _ in 0..32 {
        let Some(hit) = placed.iter().find(|o| o.key != f.key && f.avoids(o) && f.overlaps(o)) else { return true };
        let (w, h) = (f.rect[2] - f.rect[0], f.rect[3] - f.rect[1]);
        let y = f.rect[1];
        if hit.rect[2] + w <= page_w + EPS {
            f.moved_to(hit.rect[2], y);
        } else if hit.rect[0] - w >= -EPS {
            f.moved_to(hit.rect[0] - w, y);
        } else if hit.rect[3] + h <= body_bottom + EPS {
            f.moved_to(original.0, hit.rect[3]);
        } else {
            f.moved_to(original.0, original.1);
            return false;
        }
    }
    false
}

/// Horizontal position (FLOAT-6): `relativeFrom` column / margin / page, `align` left/center/right or an offset.
pub fn h_pos(f: &Float, fr: &Frames) -> f64 {
    let (x0, w) = match f.h_relative.as_str() {
        "page" => (0.0, fr.page_w),
        "margin" => (fr.margin[0], fr.page_w - fr.margin[0] - fr.margin[1]),
        "leftMargin" => (0.0, fr.margin[0]),
        "rightMargin" => (fr.page_w - fr.margin[1], fr.margin[1]),
        // shortcut: character / insideMargin / outsideMargin are not measured; treated as the column (FLOAT-6).
        _ => fr.column,
    };
    match f.h_align.as_deref() {
        Some("center") => x0 + (w - f.w) / 2.0,
        Some("right" | "outside") => x0 + w - f.w,
        Some(_) => x0,
        None => x0 + f.h_offset,
    }
}

/// Vertical position (FLOAT-6). `para_top` = the anchor paragraph's first line box top, `line_top` = the anchor line's.
pub fn v_pos(f: &Float, h: f64, fr: &Frames, para_top: f64, line_top: f64) -> f64 {
    let (y0, ht) = match f.v_relative.as_str() {
        "page" => (0.0, fr.page_h),
        "margin" => (fr.margin[2], fr.page_h - fr.margin[2] - fr.margin[3]),
        "topMargin" => (0.0, fr.margin[2]),
        "bottomMargin" => (fr.page_h - fr.margin[3], fr.margin[3]),
        "line" => (line_top, 0.0),
        _ => (para_top, 0.0),
    };
    match f.v_align.as_deref() {
        Some("center") => y0 + (ht - h) / 2.0,
        Some("bottom" | "outside") => y0 + ht - h,
        Some(_) => y0,
        None => y0 + f.v_offset,
    }
}

/// FLOAT-2, FLOAT-3: the free intervals of a column [x0, x1] in a band, around every square-wrapped float overlapping
/// it. Returns None when a topAndBottom float overlaps the band (the line must move below it, FLOAT-8).
pub fn intervals(floats: &[Placed], x0: f64, x1: f64, top: f64, bottom: f64) -> Option<Vec<(f64, f64)>> {
    let mut free = vec![(x0, x1)];
    for f in floats.iter().filter(|f| f.in_band(top, bottom)) {
        // Only floats overlapping the column horizontally affect it (FLOAT-11).
        if f.excl[2] <= x0 + EPS || f.excl[0] >= x1 - EPS {
            continue;
        }
        if f.wrap == Wrap::TopAndBottom {
            return None;
        }
        let (l, r) = (f.excl[0], f.excl[2]);
        let side = match f.side {
            Side::Largest => {
                // A tie goes to the left side (FLOAT-3).
                if (l - x0) >= (x1 - r) - EPS { Side::Left } else { Side::Right }
            }
            s => s,
        };
        let block = match side {
            Side::Left => (l, f64::INFINITY),
            Side::Right => (f64::NEG_INFINITY, r),
            _ => (l, r),
        };
        let mut next = Vec::new();
        for (a, b) in free {
            if block.1 <= a + EPS || block.0 >= b - EPS {
                next.push((a, b));
                continue;
            }
            if block.0 > a + EPS {
                next.push((a, block.0));
            }
            if block.1 < b - EPS {
                next.push((block.1, b));
            }
        }
        free = next;
    }
    Some(free)
}

/// FLOAT-4: the lowest edge a blocked band can move down to: the nearest exclusion bottom below `top` among floats
/// overlapping the band.
pub fn next_edge(floats: &[Placed], x0: f64, x1: f64, top: f64, bottom: f64) -> Option<f64> {
    floats
        .iter()
        .filter(|f| f.in_band(top, bottom) && f.excl[2] > x0 + EPS && f.excl[0] < x1 - EPS)
        .map(|f| f.excl[3])
        .filter(|&b| b > top + EPS)
        .fold(None, |m: Option<f64>, b| Some(m.map_or(b, |m| m.min(b))))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fl(x0: f64, x1: f64, side: Side) -> Placed {
        let float = Rc::new(Float {
            id: 1,
            w: x1 - x0,
            auto_w: false,
            h: 100.0,
            image: None,
            alt: "".into(),
            lock_aspect: false,
            text_box: None,
            h_relative: "column".into(),
            h_align: None,
            h_offset: 0.0,
            v_relative: "paragraph".into(),
            v_align: None,
            v_offset: 0.0,
            wrap: "square".into(),
            wrap_text: "bothSides".into(),
            dist: [0.0; 4],
            behind: false,
            allow_overlap: true,
            layout_in_cell: true,
            order: 0,
            src: None,
            locked: false,
            frame_src: None,
        });
        Placed {
            key: (0, 0, 0),
            anchor: ParaId(1),
            rect: [x0, 0.0, x1, 100.0],
            excl: [x0, 0.0, x1, 100.0],
            wrap: Wrap::Square,
            side,
            allow_overlap: true,
            float,
        }
    }

    #[test]
    fn sides() {
        let f = [fl(222.0, 322.0, Side::Both)];
        assert_eq!(intervals(&f, 72.0, 540.0, 10.0, 20.0).unwrap(), vec![(72.0, 222.0), (322.0, 540.0)]);
        let f = [fl(222.0, 322.0, Side::Largest)];
        assert_eq!(intervals(&f, 72.0, 540.0, 10.0, 20.0).unwrap(), vec![(322.0, 540.0)]);
        // A tie goes left.
        let f = [fl(256.0, 356.0, Side::Largest)];
        assert_eq!(intervals(&f, 72.0, 540.0, 10.0, 20.0).unwrap(), vec![(72.0, 256.0)]);
        // Touching bands don't overlap (FLOAT-1).
        let f = [fl(222.0, 322.0, Side::Both)];
        assert_eq!(intervals(&f, 72.0, 540.0, 100.0, 110.0).unwrap(), vec![(72.0, 540.0)]);
    }
}
