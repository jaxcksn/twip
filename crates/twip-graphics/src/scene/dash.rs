// Copyright 2023 the Kurbo Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Dashing a path into kept scratch
//!
//! Kurbo's dash iterator (`kurbo::dash`, kurbo 0.13.1, MIT or Apache-2.0) step for step, in its default order (a
//! closed subpath's first dash held back to join the dash that wraps through its close), with its held-back elements
//! in a buffer the caller keeps: kurbo's allocates its own for each path.

use kurbo::{CubicBez, Line, ParamCurve, ParamCurveArclen, PathEl, PathSeg, Point, QuadBez};

const DASH_ACCURACY: f64 = 1e-6;

#[derive(PartialEq, Eq)]
enum State {
    NeedInput,
    ToStash,
    Working,
    FromStash,
}

/// `inner` dashed by `dashes` from `offset`, as `kurbo::dash` dashes it.
pub(super) struct Dash<'a, I> {
    inner: I,
    input_done: bool,
    closepath_pending: bool,
    dashes: &'a [f64],
    dash_ix: usize,
    init_dash_ix: usize,
    init_dash_remaining: f64,
    init_is_active: bool,
    is_active: bool,
    state: State,
    current_seg: PathSeg,
    t: f64,
    dash_remaining: f64,
    seg_remaining: f64,
    start_pt: Point,
    last_pt: Point,
    stash: &'a mut Vec<PathEl>,
    stash_ix: usize,
    needs_moveto: bool,
}

/// `inner` dashed by `dashes` (non-negative, a finite positive sum) from `offset`, its held-back elements in `stash`.
pub(super) fn dash<'a, I: Iterator<Item = PathEl>>(inner: I, offset: f64, dashes: &'a [f64], stash: &'a mut Vec<PathEl>) -> Dash<'a, I> {
    let period: f64 = dashes.iter().sum();
    let period = if dashes.len() % 2 == 1 { 2.0 * period } else { period };
    let offset = offset.rem_euclid(period);
    let (mut dash_ix, mut dash_remaining, mut is_active) = (0, dashes[0] - offset, true);
    while dash_remaining < 0.0 {
        dash_ix = (dash_ix + 1) % dashes.len();
        dash_remaining += dashes[dash_ix];
        is_active = !is_active;
    }
    stash.clear();
    Dash {
        inner,
        input_done: false,
        closepath_pending: false,
        dashes,
        dash_ix,
        init_dash_ix: dash_ix,
        init_dash_remaining: dash_remaining,
        init_is_active: is_active,
        is_active,
        state: State::NeedInput,
        current_seg: PathSeg::Line(Line::new(Point::ORIGIN, Point::ORIGIN)),
        t: 0.0,
        dash_remaining,
        seg_remaining: 0.0,
        start_pt: Point::ORIGIN,
        last_pt: Point::ORIGIN,
        stash,
        stash_ix: 0,
        needs_moveto: true,
    }
}

impl<I: Iterator<Item = PathEl>> Iterator for Dash<'_, I> {
    type Item = PathEl;

    fn next(&mut self) -> Option<PathEl> {
        loop {
            match self.state {
                State::NeedInput => {
                    if self.input_done {
                        return None;
                    }
                    self.get_input();
                    if self.input_done {
                        return None;
                    }
                    self.state = State::ToStash;
                }
                State::ToStash => {
                    if let Some(el) = self.step() {
                        self.stash.push(el);
                    }
                }
                State::Working => {
                    if let Some(el) = self.step() {
                        return Some(el);
                    }
                }
                State::FromStash => {
                    if let Some(&el) = self.stash.get(self.stash_ix) {
                        self.stash_ix += 1;
                        return Some(el);
                    }
                    self.stash.clear();
                    self.stash_ix = 0;
                    if self.input_done {
                        return None;
                    }
                    if self.closepath_pending {
                        self.closepath_pending = false;
                        self.state = State::NeedInput;
                    } else {
                        self.state = State::ToStash;
                    }
                }
            }
        }
    }
}

fn seg_to_el(seg: &PathSeg) -> PathEl {
    match seg {
        PathSeg::Line(l) => PathEl::LineTo(l.p1),
        PathSeg::Quad(q) => PathEl::QuadTo(q.p1, q.p2),
        PathSeg::Cubic(c) => PathEl::CurveTo(c.p1, c.p2, c.p3),
    }
}

impl<I: Iterator<Item = PathEl>> Dash<'_, I> {
    fn get_input(&mut self) {
        loop {
            if self.closepath_pending {
                self.handle_closepath();
                break;
            }
            let Some(next) = self.inner.next() else {
                self.input_done = true;
                self.state = State::FromStash;
                return;
            };
            let p0 = self.last_pt;
            match next {
                PathEl::MoveTo(p) => {
                    if !self.stash.is_empty() {
                        self.state = State::FromStash;
                    }
                    self.start_pt = p;
                    self.last_pt = p;
                    self.reset_phase();
                    continue;
                }
                PathEl::LineTo(p1) => {
                    let l = Line::new(p0, p1);
                    self.seg_remaining = l.arclen(DASH_ACCURACY);
                    self.current_seg = PathSeg::Line(l);
                    self.last_pt = p1;
                }
                PathEl::QuadTo(p1, p2) => {
                    let q = QuadBez::new(p0, p1, p2);
                    self.seg_remaining = q.arclen(DASH_ACCURACY);
                    self.current_seg = PathSeg::Quad(q);
                    self.last_pt = p2;
                }
                PathEl::CurveTo(p1, p2, p3) => {
                    let c = CubicBez::new(p0, p1, p2, p3);
                    self.seg_remaining = c.arclen(DASH_ACCURACY);
                    self.current_seg = PathSeg::Cubic(c);
                    self.last_pt = p3;
                }
                PathEl::ClosePath => {
                    self.closepath_pending = true;
                    if p0 != self.start_pt {
                        let l = Line::new(p0, self.start_pt);
                        self.seg_remaining = l.arclen(DASH_ACCURACY);
                        self.current_seg = PathSeg::Line(l);
                        self.last_pt = self.start_pt;
                    } else {
                        self.handle_closepath();
                    }
                }
            }
            break;
        }
        self.t = 0.0;
    }

    /// The next element as far as the next dash transition or segment end.
    fn step(&mut self) -> Option<PathEl> {
        let mut result = None;
        if self.state == State::ToStash && self.needs_moveto {
            self.needs_moveto = false;
            if self.is_active {
                result = Some(PathEl::MoveTo(self.current_seg.start()));
            } else {
                self.state = State::Working;
            }
        } else if self.dash_remaining < self.seg_remaining {
            let seg = self.current_seg.subsegment(self.t..1.0);
            let t1 = seg.inv_arclen(self.dash_remaining, DASH_ACCURACY);
            if self.is_active {
                result = Some(seg_to_el(&seg.subsegment(0.0..t1)));
                self.state = State::Working;
            } else {
                result = Some(PathEl::MoveTo(seg.eval(t1)));
            }
            self.is_active = !self.is_active;
            self.t += t1 * (1.0 - self.t);
            self.seg_remaining -= self.dash_remaining;
            self.dash_ix += 1;
            if self.dash_ix == self.dashes.len() {
                self.dash_ix = 0;
            }
            self.dash_remaining = self.dashes[self.dash_ix];
        } else {
            if self.is_active {
                result = Some(seg_to_el(&self.current_seg.subsegment(self.t..1.0)));
            }
            self.dash_remaining -= self.seg_remaining;
            self.get_input();
        }
        result
    }

    fn handle_closepath(&mut self) {
        if self.state == State::ToStash {
            self.stash.push(PathEl::ClosePath);
        } else if self.is_active {
            self.stash_ix = 1;
        }
        self.state = State::FromStash;
        self.reset_phase();
    }

    fn reset_phase(&mut self) {
        self.dash_ix = self.init_dash_ix;
        self.dash_remaining = self.init_dash_remaining;
        self.is_active = self.init_is_active;
        self.needs_moveto = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::{BezPath, Shape};

    /// Paths open and closed, of lines, quads and cubics, a contour shorter than its first dash, one that starts in a
    /// gap, and a zero-length close, under patterns odd and even and offsets past a period.
    #[test]
    fn dashes_as_kurbo_does() {
        let mut paths = vec![
            kurbo::Circle::new((20.0, 20.0), 9.0).to_path(0.01),
            kurbo::RoundedRect::new(0.0, 0.0, 40.0, 25.0, 6.0).to_path(0.01),
            kurbo::Rect::new(0.0, 0.0, 1.0, 1.0).to_path(0.01),
            kurbo::Ellipse::new((5.0, 5.0), (30.0, 4.0), 0.4).to_path(0.01),
        ];
        let mut open = BezPath::new();
        open.move_to((0.0, 0.0));
        open.quad_to((10.0, 20.0), (20.0, 0.0));
        open.curve_to((25.0, -10.0), (35.0, 10.0), (40.0, 0.0));
        open.move_to((0.0, 30.0));
        open.line_to((50.0, 30.0));
        open.line_to((50.0, 30.0));
        paths.push(open);
        let mut closed_at_start = BezPath::new();
        closed_at_start.move_to((0.0, 0.0));
        closed_at_start.line_to((30.0, 0.0));
        closed_at_start.line_to((30.0, 20.0));
        closed_at_start.line_to((0.0, 0.0));
        closed_at_start.close_path();
        paths.push(closed_at_start);
        let patterns: [&[f64]; 5] = [&[2.0, 1.0], &[1.0, 0.5, 2.0, 0.5, 0.25, 0.5], &[3.0], &[0.0, 2.0], &[100.0, 1.0]];
        let mut stash = Vec::new();
        for path in &paths {
            for pattern in patterns {
                for offset in [0.0, 0.7, 2.5, -1.3, 41.0] {
                    let theirs: Vec<PathEl> = kurbo::dash(path.iter(), offset, pattern).collect();
                    let ours: Vec<PathEl> = dash(path.iter(), offset, pattern, &mut stash).collect();
                    assert_eq!(ours, theirs, "{pattern:?} from {offset}");
                }
            }
        }
    }
}
