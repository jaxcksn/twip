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
/// A table in a vertical section: drawn upright, an obstacle on the turned page.
pub const UPRIGHT_TABLE: usize = usize::MAX - 1;

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

#[derive(Clone, Debug, PartialEq)]
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
    pub contour: Option<Contour>,
    /// The distances text keeps from it: top, bottom, left, right.
    pub dist: [f64; 4],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Contour {
    pub points: Rc<[(f64, f64)]>,
    pub through: bool,
}

pub struct Frames {
    pub page_w: f64,
    pub page_h: f64,
    pub margin: [f64; 4],
    pub column: (f64, f64),
    pub compat: Compat,
    pub cell: bool,
    pub odd: bool,
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
        let contour = f
            .poly
            .as_ref()
            .filter(|_| wrap == Wrap::Square)
            .map(|p| Contour { points: p.iter().map(|&(px, py)| (x + px * f.w, y + py * h)).collect(), through: f.wrap == "through" });
        let wrap_box = match &contour {
            Some(c) => {
                let b =
                    c.points.iter().fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |b, &(x, y)| [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)]);
                [b[0], b[1], b[2], b[3].max(b[1] + 1.0 / LU_PER_PT)]
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
            contour,
            dist: f.dist,
        }
    }

    /// Compat ≤ 14 keeps no distance above or below a tight or through float.
    pub fn legacy_dist(mut self) -> Placed {
        if matches!(self.float.wrap.as_str(), "tight" | "through") {
            let [dt, db, ..] = self.dist;
            self.excl[1] += dt;
            self.excl[3] -= db;
            self.dist[0] = 0.0;
            self.dist[1] = 0.0;
        }
        self
    }

    /// The narrowest piece of a line it leaves beside it: in compat ≤ 14 an inch beside a frame and half the usual
    /// beside a tight or through float.
    pub fn gap(&self, legacy: bool) -> f64 {
        let frame = self.float.text_box.is_some() && self.float.shape.is_none() && self.key.2 != TABLE_PIECE;
        match () {
            _ if !legacy => 18.0,
            _ if frame => 72.0,
            _ if self.key.2 != TABLE_PIECE && matches!(self.float.wrap.as_str(), "tight" | "through") => 9.0,
            _ => 18.0,
        }
    }

    /// Wraps around a text box's lines instead of its box: one rectangle per line, from the box's top left.
    pub fn around_lines(mut self, lines: &[[f64; 4]]) -> Placed {
        if lines.is_empty() {
            return self;
        }
        let (x, y) = (self.rect[0], self.rect[1]);
        let down = lines.iter().flat_map(|r| [(x + r[2], y + r[1]), (x + r[2], y + r[3])]);
        let up = lines.iter().rev().flat_map(|r| [(x + r[0], y + r[3]), (x + r[0], y + r[1])]);
        let points: Rc<[(f64, f64)]> = down.chain(up).collect();
        let b = points.iter().fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |b, &(x, y)| [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)]);
        let [dt, db, dl, dr] = self.dist;
        self.excl = [b[0] - dl, b[1] - dt, b[2] + dr, b[3] + db];
        self.contour = Some(Contour { points, through: false });
        self
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
        if let Some(c) = &mut self.contour {
            c.points = c.points.iter().map(|&(x, y)| (x + dx, y + dy)).collect();
        }
    }

    fn avoids(&self, o: &Placed) -> bool {
        (!self.allow_overlap || !o.allow_overlap) && ((self.wrap == Wrap::None) == (o.wrap == Wrap::None))
    }

    /// The room a frame or a floating table keeps around it from other objects that may not overlap it; a
    /// picture keeps none.
    fn apart(&self) -> [f64; 4] {
        let frame = self.float.text_box.is_some() && self.float.shape.is_none();
        if frame || self.key.2 == TABLE_PIECE { self.float.dist } else { [0.0; 4] }
    }

    /// The two boxes, each grown by the room it keeps, overlap.
    fn overlaps(&self, o: &Placed) -> bool {
        let ([st, sb, sl, sr], [ot, ob, ol, or]) = (self.apart(), o.apart());
        self.rect[0] - sl < o.rect[2] + or - EPS
            && o.rect[0] - ol < self.rect[2] + sr - EPS
            && self.rect[1] - st < o.rect[3] + ob - EPS
            && o.rect[1] - ot < self.rect[3] + sb - EPS
    }
}

/// Moves a float that may not overlap an earlier one clear of it, the two kept apart by the room each keeps:
/// compat 15 to the nearer side on the same line (right up to the page's edge, left down to the left margin),
/// compat ≤ 14 to the right while it fits the page, else down.
pub fn avoid(f: &mut Placed, placed: &[Placed], (left_margin, page_w): (f64, f64), body_bottom: f64, compat: Compat) -> bool {
    let original = (f.rect[0], f.rect[1]);
    for _ in 0..32 {
        let Some(hit) = placed.iter().find(|o| o.key != f.key && f.avoids(o) && f.overlaps(o)) else { return true };
        let (w, h) = (f.rect[2] - f.rect[0], f.rect[3] - f.rect[1]);
        let ([ft, _, fl, fr], [_, hb, hl, hr]) = (f.apart(), hit.apart());
        let (x, y) = (f.rect[0], f.rect[1]);
        let right = Some(hit.rect[2] + hr + fl).filter(|&r| r + w <= page_w + EPS);
        let left = Some(hit.rect[0] - hl - fr - w).filter(|&l| l >= left_margin - EPS && compat == Compat::Generic);
        let side = match (right, left) {
            (Some(r), Some(l)) => Some(if r - x <= x - l { r } else { l }),
            (r, l) => r.or(l),
        };
        if let Some(to) = side {
            f.moved_to(to, y);
        } else if hit.rect[3] + hb + ft + h <= body_bottom + EPS {
            f.moved_to(original.0, hit.rect[3] + hb + ft);
        } else {
            f.moved_to(original.0, original.1);
            return false;
        }
    }
    false
}

pub use fit::{intervals, next_edge, steps};
pub use position::{h_pos, sized, v_pos};

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
            relative: Default::default(),
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
            contour: None,
            dist: [0.0; 4],
        }
    }

    #[test]
    fn sides() {
        let f = [fl(222.0, 322.0, Side::Both)];
        assert_eq!(intervals(&f, 72.0, 540.0, 10.0, 20.0, false).unwrap(), vec![(72.0, 222.0), (322.0, 540.0)]);
        let f = [fl(222.0, 322.0, Side::Largest)];
        assert_eq!(intervals(&f, 72.0, 540.0, 10.0, 20.0, false).unwrap(), vec![(322.0, 540.0)]);
        let f = [fl(256.0, 356.0, Side::Largest)];
        assert_eq!(intervals(&f, 72.0, 540.0, 10.0, 20.0, false).unwrap(), vec![(72.0, 256.0)]);
        let f = [fl(222.0, 322.0, Side::Both)];
        assert_eq!(intervals(&f, 72.0, 540.0, 100.0, 110.0, false).unwrap(), vec![(72.0, 540.0)]);
        let f = [fl(89.0, 522.0, Side::Both)];
        assert_eq!(intervals(&f, 72.0, 540.0, 10.0, 20.0, false).unwrap(), vec![(522.0, 540.0)]);
    }

    #[test]
    fn relative_sizes_take_their_share_of_the_page_or_margins() {
        let fr = Frames {
            page_w: 612.0,
            page_h: 792.0,
            margin: [72.0, 36.0, 72.0, 144.0],
            column: (72.0, 504.0),
            compat: Compat::Generic,
            cell: false,
            odd: true,
        };
        let size = |w_from: &str, h_from: &str| {
            let mut f = (*fl(0.0, 40.0, Side::Both).float).clone();
            f.relative.size = [Some((w_from.into(), 500)), Some((h_from.into(), 500))];
            let s = sized(&f, &fr).expect("sized");
            (s.w, s.h)
        };
        assert_eq!(size("margin", "margin"), (252.0, 288.0));
        assert_eq!(size("page", "page"), (306.0, 396.0));
        assert_eq!(size("leftMargin", "topMargin"), (36.0, 36.0));
        assert_eq!(size("rightMargin", "bottomMargin"), (18.0, 72.0));
        assert_eq!(size("insideMargin", "insideMargin"), (18.0, 36.0));
        let mut f = (*fl(0.0, 40.0, Side::Both).float).clone();
        f.relative.size = [Some(("page".into(), 500)), None];
        f.effect = [10.0, 0.0, 20.0, 0.0];
        assert_eq!(sized(&f, &fr).expect("sized").w, 276.0);
        f.relative.size = [None, Some(("page".into(), 1))];
        assert_eq!(sized(&f, &fr).expect("sized").h, 1.0);
        let cell = Frames { cell: true, ..fr };
        assert!(sized(&f, &cell).is_none());
    }

    #[test]
    fn percentage_positions_count_from_their_reference_box() {
        let fr = Frames { page_w: 612.0, page_h: 792.0, margin: [72.0; 4], column: (72.0, 468.0), compat: Compat::Generic, cell: false, odd: true };
        let at = |h: &str, pct: i64| {
            let mut f = (*fl(0.0, 40.0, Side::Both).float).clone();
            f.h_relative = h.into();
            f.relative.pos = [Some(pct), None];
            h_pos(&f, &fr)
        };
        assert_eq!((at("margin", 250), at("page", 250), at("leftMargin", 250)), (189.0, 153.0, 18.0));
        let two_columns = Frames { column: (72.0, 216.0), ..fr };
        let mut f = (*fl(0.0, 40.0, Side::Both).float).clone();
        f.relative.pos = [Some(500), None];
        assert_eq!(h_pos(&f, &two_columns), 306.0);
        f.v_relative = "page".into();
        f.relative.pos = [None, Some(123)];
        assert!((v_pos(&f, 20.0, &fr, 0.0, 0.0) - 97.42).abs() < 0.05);
    }

    fn polygon(wrap: &str, pts: &[(f64, f64)], dist: [f64; 4]) -> Placed {
        let mut f = (*fl(0.0, 200.0, Side::Both).float).clone();
        f.wrap = wrap.into();
        f.poly = Some(pts.iter().map(|&(x, y)| (x / 21600.0, y / 21600.0)).collect());
        f.dist = dist;
        Placed::new((0, 0, 0), ParaId(1), &Rc::new(f), 72.0, 72.0, 100.0)
    }

    #[test]
    fn a_polygon_wraps_by_its_widest_point_over_the_band() {
        let tri = [(0.0, 0.0), (0.0, 21600.0), (21600.0, 21600.0)];
        let starts = |dist| intervals(&[polygon("tight", &tri, dist)], 72.0, 540.0, 72.0, 83.49, false).unwrap();
        let free = starts([0.0; 4]);
        assert!((free[0].0 - 94.98).abs() < 1e-6 && free.len() == 1, "{free:?}");
        let free = starts([0.0, 0.0, 0.0, 18.0]);
        assert!((free[0].0 - 112.98).abs() < 1e-6, "{free:?}");
        assert_eq!(intervals(&[polygon("tight", &tri, [0.0; 4])], 72.0, 540.0, 180.0, 190.0, false).unwrap(), vec![(72.0, 540.0)]);
    }

    #[test]
    fn a_through_wrap_opens_a_notch_that_a_tight_wrap_keeps_closed() {
        let u =
            [(0.0, 0.0), (7200.0, 0.0), (7200.0, 14400.0), (14400.0, 14400.0), (14400.0, 0.0), (21600.0, 0.0), (21600.0, 21600.0), (0.0, 21600.0)];
        let band = |wrap, top: f64| intervals(&[polygon(wrap, &u, [0.0; 4])], 0.0, 540.0, top, top + 11.49, false).unwrap();
        let notch = (72.0 + 200.0 / 3.0, 72.0 + 400.0 / 3.0);
        let through = band("through", 80.0);
        assert!(through.iter().any(|&(a, b)| (a - notch.0).abs() < 1e-6 && (b - notch.1).abs() < 1e-6), "{through:?}");
        assert!(!band("tight", 80.0).iter().any(|&(a, _)| a > 72.0 && a < 272.0));
        assert!(!band("through", 135.0).iter().any(|&(a, _)| a > 72.0 && a < 272.0), "the band over the notch bottom");
    }

    #[test]
    fn inside_and_outside_follow_the_page_parity() {
        let fr = |compat, odd| Frames { page_w: 612.0, page_h: 792.0, margin: [72.0; 4], column: (72.0, 468.0), compat, cell: false, odd };
        let x = |rel: &str, align: &str, compat, odd| {
            let mut f = (*fl(0.0, 40.0, Side::Both).float).clone();
            f.h_relative = rel.into();
            f.h_align = Some(align.into());
            h_pos(&f, &fr(compat, odd))
        };
        assert_eq!((x("insideMargin", "left", Compat::Generic, true), x("insideMargin", "left", Compat::Generic, false)), (0.0, 540.0));
        assert_eq!((x("insideMargin", "left", Compat::Legacy, true), x("outsideMargin", "left", Compat::Legacy, true)), (540.0, 0.0));
        assert_eq!((x("margin", "inside", Compat::Generic, true), x("margin", "inside", Compat::Generic, false)), (72.0, 500.0));
        let even = Frames { margin: [72.0, 36.0, 72.0, 144.0], ..fr(Compat::Generic, false) };
        let mut f = (*fl(0.0, 40.0, Side::Both).float).clone();
        f.relative.size = [Some(("insideMargin".into(), 500)), Some(("insideMargin".into(), 500))];
        let s = sized(&f, &even).expect("sized");
        assert_eq!((s.w, s.h), (36.0, 72.0));
    }

    #[test]
    fn compat_14_snaps_an_aligned_offset_to_whole_twips() {
        let fr = |compat| Frames { page_w: 612.0, page_h: 792.0, margin: [72.0; 4], column: (72.0, 468.0), compat, cell: false, odd: true };
        let mut f = (*fl(0.0, 40.03, Side::Both).float).clone();
        f.h_relative = "margin".into();
        f.h_align = Some("center".into());
        assert_eq!(h_pos(&f, &fr(Compat::Legacy)), 286.0);
        assert!((h_pos(&f, &fr(Compat::Generic)) - 285.985).abs() < 1e-9);
    }

    #[test]
    fn positions() {
        let fr =
            |compat| Frames { page_w: 612.0, page_h: 792.0, margin: [72.0, 72.0, 72.0, 72.0], column: (72.0, 468.0), compat, cell: false, odd: true };
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

    #[test]
    fn a_piece_narrower_than_its_floats_gap_gets_no_text() {
        let with = |wrap: &str, frame: bool| {
            let mut f = (*fl(135.0, 300.0, Side::Both).float).clone();
            f.wrap = wrap.into();
            if frame {
                f.text_box = Some(crate::docx::TextBox { blocks: Vec::new(), insets: [0.0; 4], outline: 0.0, auto_fit: false, v_anchor: 0, flow: 0 });
            }
            [Placed::new((0, 0, 0), ParaId(1), &Rc::new(f), 135.0, 0.0, 100.0)]
        };
        let left = |f: &[Placed], legacy| intervals(f, 72.0, 540.0, 10.0, 20.0, legacy).unwrap()[0];
        assert_eq!(left(&with("square", false), true), (72.0, 135.0));
        assert_eq!(left(&with("square", true), true), (300.0, 540.0), "an inch beside a compat 14 frame");
        assert_eq!(left(&with("square", true), false), (72.0, 135.0));
        let narrow = |wrap: &str, legacy| intervals(&with(wrap, false), 120.0, 540.0, 10.0, 20.0, legacy).unwrap()[0];
        assert_eq!(narrow("tight", true), (120.0, 135.0), "9 pt beside a compat 14 tight float");
        assert_eq!(narrow("tight", false), (300.0, 540.0));
    }

    #[test]
    fn compat_14_keeps_no_distance_above_or_below_a_tight_float() {
        let square = [(0.0, 0.0), (21600.0, 0.0), (21600.0, 21600.0), (0.0, 21600.0)];
        let p = polygon("tight", &square, [10.0, 10.0, 5.0, 5.0]).legacy_dist();
        assert_eq!(p.excl, [67.0, 72.0, 277.0, 172.0]);
        assert!(intervals(&[p], 0.0, 540.0, 175.0, 185.0, true).unwrap() == vec![(0.0, 540.0)]);
        let q = polygon("square", &square, [10.0, 10.0, 5.0, 5.0]).legacy_dist();
        assert_eq!(q.excl[1], 62.0);
    }
}
