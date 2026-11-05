//! Snapping

use super::view::{Grid, Pt};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SnapKind {
    Margin,
    PageCentre,
    TableEdge,
    ObjectEdge,
    ObjectCentre,
    Grid,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SnapLine {
    pub at: Pt,
    pub kind: SnapKind,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Snapping {
    #[default]
    On,
    Free,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Sources<'a> {
    pub size: [f64; 2],
    pub margins: [f64; 4],
    pub tables: &'a [[f64; 4]],
    pub objects: &'a [[f64; 4]],
    pub grid: Option<Grid>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct SnapLines {
    pub x: Vec<SnapLine>,
    pub y: Vec<SnapLine>,
    pub grid: Option<Grid>,
    pub size: [f64; 2],
    pub margins: [f64; 4],
}

impl SnapLines {
    pub(crate) fn fill(&mut self, s: &Sources) {
        let line = |at: f64, kind| SnapLine { at: Pt(at), kind };
        let [l, t, r, b] = s.margins;
        self.x.clear();
        self.y.clear();
        self.x.extend([line(l, SnapKind::Margin), line(r, SnapKind::Margin), line(s.size[0] / 2.0, SnapKind::PageCentre)]);
        self.y.extend([line(t, SnapKind::Margin), line(b, SnapKind::Margin), line(s.size[1] / 2.0, SnapKind::PageCentre)]);
        for &[x0, y0, x1, y1] in s.tables {
            self.x.extend([line(x0, SnapKind::TableEdge), line(x1, SnapKind::TableEdge)]);
            self.y.extend([line(y0, SnapKind::TableEdge), line(y1, SnapKind::TableEdge)]);
        }
        for &[x0, y0, x1, y1] in s.objects {
            self.x.extend([line(x0, SnapKind::ObjectEdge), line(x1, SnapKind::ObjectEdge), line((x0 + x1) / 2.0, SnapKind::ObjectCentre)]);
            self.y.extend([line(y0, SnapKind::ObjectEdge), line(y1, SnapKind::ObjectEdge), line((y0 + y1) / 2.0, SnapKind::ObjectCentre)]);
        }
        (self.grid, self.size, self.margins) = (s.grid, s.size, s.margins);
    }
}

pub(crate) fn turned_bounds(r: [f64; 4], deg: f64) -> [f64; 4] {
    let (s, c) = deg.to_radians().sin_cos();
    let (hw, hh) = ((r[2] - r[0]) / 2.0, (r[3] - r[1]) / 2.0);
    let (ex, ey) = (hw * c.abs() + hh * s.abs(), hw * s.abs() + hh * c.abs());
    let (cx, cy) = ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
    [cx - ex, cy - ey, cx + ex, cy + ey]
}

fn nearest(lines: &[SnapLine], lo: f64, hi: f64, threshold: f64) -> Option<(f64, SnapLine)> {
    let mut best: Option<(f64, SnapLine)> = None;
    for l in lines {
        for p in [lo, (lo + hi) / 2.0, hi] {
            let d = l.at.0 - p;
            if d.abs() <= threshold && best.is_none_or(|(bd, bl)| (d.abs(), l.kind) < (bd.abs(), bl.kind)) {
                best = Some((d, *l));
            }
        }
    }
    best
}

fn on_grid(lo: f64, origin: Pt, step: Pt) -> (f64, SnapLine) {
    let at = origin.0 + ((lo - origin.0) / step.0).round() * step.0;
    (at - lo, SnapLine { at: Pt(at), kind: SnapKind::Grid })
}

pub(crate) fn snap(b: [f64; 4], lines: &SnapLines, threshold: Pt) -> ([f64; 2], [Option<SnapLine>; 2]) {
    let axis = |set: &[SnapLine], lo: f64, hi: f64, k: usize| {
        nearest(set, lo, hi, threshold.0).or_else(|| lines.grid.map(|g| on_grid(lo, g.origin[k], g.step)))
    };
    let (x, y) = (axis(&lines.x, b[0], b[2], 0), axis(&lines.y, b[1], b[3], 1));
    ([x.map_or(0.0, |s| s.0), y.map_or(0.0, |s| s.0)], [x.map(|s| s.1), y.map(|s| s.1)])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(grid: Option<Grid>) -> SnapLines {
        let mut l = SnapLines::default();
        l.fill(&Sources {
            size: [612.0, 792.0],
            margins: [72.0, 72.0, 540.0, 720.0],
            tables: &[[72.0, 200.0, 400.0, 300.0]],
            objects: &[[420.0, 400.0, 500.0, 480.0]],
            grid,
        });
        l
    }

    #[test]
    fn the_nearest_line_wins() {
        let (off, hit) = snap([75.0, 500.0, 399.0, 560.0], &page(None), Pt(5.25));
        assert_eq!(off[0], 1.0);
        assert_eq!(hit[0], Some(SnapLine { at: Pt(400.0), kind: SnapKind::TableEdge }));
        let (off, hit) = snap([255.0, 500.0, 355.0, 560.0], &page(None), Pt(5.25));
        assert_eq!((off[0], hit[0].map(|h| h.kind)), (1.0, Some(SnapKind::PageCentre)));
    }

    #[test]
    fn ties_go_to_the_margins_first() {
        let (off, hit) = snap([70.0, 500.0, 170.0, 560.0], &page(None), Pt(5.25));
        assert_eq!(off[0], 2.0);
        assert_eq!(hit[0].map(|h| h.kind), Some(SnapKind::Margin));
    }

    #[test]
    fn nothing_within_the_threshold_leaves_the_box() {
        let (off, hit) = snap([100.0, 500.0, 150.0, 560.0], &page(None), Pt(5.25));
        assert_eq!((off, hit), ([0.0, 0.0], [None, None]));
    }

    #[test]
    fn the_grid_applies_only_where_no_line_does() {
        let grid = Some(Grid { origin: [Pt(72.0), Pt(72.0)], step: Pt(18.0) });
        let (off, hit) = snap([100.0, 600.0, 150.0, 717.0], &page(grid), Pt(5.25));
        assert_eq!(off, [8.0, 3.0]);
        assert_eq!(hit[0], Some(SnapLine { at: Pt(108.0), kind: SnapKind::Grid }));
        assert_eq!(hit[1].map(|h| h.kind), Some(SnapKind::Margin));
    }

    #[test]
    fn a_turned_object_snaps_by_its_bounds() {
        let b = turned_bounds([80.0, 90.0, 120.0, 110.0], 90.0);
        assert!(b.iter().zip([90.0, 80.0, 110.0, 120.0]).all(|(a, e)| (a - e).abs() < 1e-9), "{b:?}");
        let mut l = SnapLines::default();
        l.fill(&Sources { size: [612.0, 792.0], margins: [72.0, 72.0, 540.0, 720.0], tables: &[], objects: &[b], grid: None });
        let (off, hit) = snap([112.0, 300.0, 150.0, 340.0], &l, Pt(5.25));
        assert_eq!((off[0], hit[0].map(|h| h.kind)), (-2.0, Some(SnapKind::ObjectEdge)));
    }
}
