//! Floating objects

use std::rc::Rc;

use crate::docx::Compat;
use crate::docx::model::Float;

use super::ids::ParaId;
use super::units::LU_PER_PT;

mod fit;
mod position;

const EPS: f64 = 1e-6;

pub const TABLE_PIECE: usize = usize::MAX;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wrap {
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

#[derive(Clone, Debug)]
pub struct Placed {
    pub key: (usize, usize, usize),
    pub anchor: ParaId,
    pub rect: [f64; 4],
    pub excl: [f64; 4],
    pub wrap: Wrap,
    pub side: Side,
    pub allow_overlap: bool,
    pub float: Rc<Float>,
    pub settled: bool,
}

pub struct Frames {
    pub page_w: f64,
    pub page_h: f64,
    pub margin: [f64; 4],
    pub column: (f64, f64),
    pub compat: Compat,
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
        let wrap_box = match f.poly {
            Some([x0, y0, x1, y1]) => {
                let top = y + y0 * h;
                [x + x0 * f.w, top, x + x1 * f.w, (y + y1 * h).max(top + 1.0 / LU_PER_PT)]
            }
            None => {
                let [l, t, r, b] = f.effect;
                [rect[0] - l, rect[1] - t, rect[2] + r, rect[3] + b]
            }
        };
        Placed {
            key,
            anchor,
            rect,
            excl: [wrap_box[0] - dl, wrap_box[1] - dt, wrap_box[2] + dr, wrap_box[3] + db],
            wrap,
            side,
            allow_overlap: f.allow_overlap,
            float: f.clone(),
            settled: true,
        }
    }

    pub fn same(&self, o: &Placed) -> bool {
        self.key == o.key && self.rect.iter().zip(&o.rect).all(|(a, b)| (a - b).abs() < 1e-3)
    }

    pub fn in_band(&self, top: f64, bottom: f64) -> bool {
        self.wrap != Wrap::None && top < self.excl[3] - EPS && self.excl[1] < bottom - EPS
    }

    fn moved_to(&mut self, x: f64, y: f64) {
        let (dx, dy) = (x - self.rect[0], y - self.rect[1]);
        for (k, d) in [dx, dy, dx, dy].iter().enumerate() {
            self.rect[k] += d;
            self.excl[k] += d;
        }
    }

    fn avoids(&self, o: &Placed) -> bool {
        (!self.allow_overlap || !o.allow_overlap) && ((self.wrap == Wrap::None) == (o.wrap == Wrap::None))
    }

    fn overlaps(&self, o: &Placed) -> bool {
        self.rect[0] < o.rect[2] - EPS && o.rect[0] < self.rect[2] - EPS && self.rect[1] < o.rect[3] - EPS && o.rect[1] < self.rect[3] - EPS
    }
}

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

pub use fit::{intervals, next_edge};
pub use position::{h_pos, v_pos};

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
            effect: [0.0; 4],
            poly: None,
            behind: false,
            allow_overlap: true,
            layout_in_cell: true,
            order: 0,
            z: 0,
            src: None,
            locked: false,
            frame_src: None,
            parts: Vec::new(),
            shape: None,
            rot_swap: false,
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
            settled: true,
        }
    }

    #[test]
    fn sides() {
        let f = [fl(222.0, 322.0, Side::Both)];
        assert_eq!(intervals(&f, 72.0, 540.0, 10.0, 20.0).unwrap(), vec![(72.0, 222.0), (322.0, 540.0)]);
        let f = [fl(222.0, 322.0, Side::Largest)];
        assert_eq!(intervals(&f, 72.0, 540.0, 10.0, 20.0).unwrap(), vec![(322.0, 540.0)]);
        let f = [fl(256.0, 356.0, Side::Largest)];
        assert_eq!(intervals(&f, 72.0, 540.0, 10.0, 20.0).unwrap(), vec![(72.0, 256.0)]);
        let f = [fl(222.0, 322.0, Side::Both)];
        assert_eq!(intervals(&f, 72.0, 540.0, 100.0, 110.0).unwrap(), vec![(72.0, 540.0)]);
        let f = [fl(89.0, 522.0, Side::Both)];
        assert_eq!(intervals(&f, 72.0, 540.0, 10.0, 20.0).unwrap(), vec![(522.0, 540.0)]);
    }

    #[test]
    fn positions() {
        let fr = |compat| Frames { page_w: 612.0, page_h: 792.0, margin: [72.0, 72.0, 72.0, 72.0], column: (72.0, 468.0), compat };
        let f = |h: &str, ha: Option<&str>, v: &str, va: Option<&str>| Float {
            h_relative: h.into(),
            h_align: ha.map(Into::into),
            v_relative: v.into(),
            v_align: va.map(Into::into),
            ..(*fl(0.0, 40.0, Side::Both).float).clone()
        };
        let x = |ha, c| h_pos(&f("page", Some(ha), "page", None), &fr(c));
        assert_eq!((x("left", Compat::Legacy), x("right", Compat::Legacy)), (32.0, 540.0));
        assert_eq!((x("left", Compat::Generic), x("right", Compat::Generic)), (0.0, 572.0));
        for va in ["top", "center", "bottom"] {
            assert_eq!(v_pos(&f("column", None, "paragraph", Some(va)), 40.0, &fr(Compat::Generic), 200.0, 220.0), 200.0);
        }
    }
}
