//! Line ends

use crate::graphic::paint::{EndKind, EndSize, LineEnd};

/// A line end drawn at a line's end, its tip at the origin and the line running along +x.
#[derive(Clone, Debug, PartialEq)]
pub struct EndShape {
    /// What is drawn.
    pub outline: EndOutline,
    /// How far the line is cut back from its end, so it stops inside the end.
    pub cut: f64,
}

/// A line end's outline, in points.
#[derive(Clone, Debug, PartialEq)]
pub enum EndOutline {
    /// A filled polygon.
    Polygon(Vec<[f64; 2]>),
    /// A filled oval about the origin.
    Oval {
        /// Its radius along the line.
        rx: f64,
        /// Its radius across the line.
        ry: f64,
    },
    /// An open polyline, stroked as the line is.
    Open(Vec<[f64; 2]>),
}

fn size(s: EndSize) -> f64 {
    match s {
        EndSize::Small => 2.0,
        EndSize::Medium => 3.0,
        EndSize::Large => 5.0,
    }
}

/// The end `e` on a line `w` points wide.
pub fn end_shape(e: LineEnd, w: f64) -> EndShape {
    let w = w.max(2.0);
    let (l, hw) = (size(e.length) * w, size(e.width) * w / 2.0);
    match e.kind {
        EndKind::Triangle => {
            let t = w / 2.0;
            EndShape { outline: EndOutline::Polygon(vec![[0.0, 0.0], [l, hw], [l, -hw]]), cut: l - t }
        }
        EndKind::Stealth => {
            let notch = w * match e.length {
                EndSize::Small => 1.0,
                EndSize::Medium => 2.0,
                EndSize::Large => 3.0,
            };
            EndShape { outline: EndOutline::Polygon(vec![[0.0, 0.0], [l, hw], [notch, 0.0], [l, -hw]]), cut: notch }
        }
        EndKind::Diamond => EndShape { outline: EndOutline::Polygon(vec![[-l / 2.0, 0.0], [0.0, hw], [l / 2.0, 0.0], [0.0, -hw]]), cut: 0.0 },
        EndKind::Oval => EndShape { outline: EndOutline::Oval { rx: l / 2.0, ry: hw }, cut: 0.0 },
        EndKind::Arrow => EndShape { outline: EndOutline::Open(vec![[l, hw], [0.0, 0.0], [l, -hw]]), cut: w },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ends_take_their_size_from_the_line_width() {
        let e = |kind, ws, ls| end_shape(LineEnd { kind, width: ws, length: ls }, 2.0);
        let t = e(EndKind::Triangle, EndSize::Small, EndSize::Medium);
        assert_eq!(t.outline, EndOutline::Polygon(vec![[0.0, 0.0], [6.0, 2.0], [6.0, -2.0]]));
        assert_eq!(t.cut, 5.0);
        let s = e(EndKind::Stealth, EndSize::Large, EndSize::Small);
        assert_eq!((s.outline, s.cut), (EndOutline::Polygon(vec![[0.0, 0.0], [4.0, 5.0], [2.0, 0.0], [4.0, -5.0]]), 2.0));
        let s = e(EndKind::Stealth, EndSize::Small, EndSize::Large);
        assert_eq!(s.cut, 6.0);
        assert_eq!(e(EndKind::Oval, EndSize::Medium, EndSize::Large).outline, EndOutline::Oval { rx: 5.0, ry: 3.0 });
    }
}
