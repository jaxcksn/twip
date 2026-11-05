//! Clipped cell text

use crate::layout::Unit;

use super::Editor;
use super::geometry::GCell;
use super::position::Position;

type Hidden = Vec<(crate::layout::ParaId, Unit, Unit)>;

impl Editor {
    pub fn caret_hidden(&mut self) -> Option<&'static str> {
        let f = self.selection.focus()?;
        let inside = self.clipped_at(f).is_some_and(|(h, k, ..)| f.para_offset().is_some_and(|(_, u)| u > h[k].1 || h[k].1 == Unit(0)));
        if !inside && self.caret_view().is_some() {
            return None;
        }
        self.clipped_at(f).map(|_| "clipped")
    }

    pub(super) fn clipped_line_edge(&mut self, p: Position, end: bool) -> Option<Position> {
        let (hidden, k, ..) = self.clipped_at(p)?;
        let (para, a, b) = hidden[k];
        Some(Position::text(p.story, para, if end { b } else { a }))
    }

    pub(super) fn clipped_x(&mut self, p: Position) -> Option<f64> {
        let (_, _, page, chain) = self.clipped_at(p)?;
        let geo = self.pages.page(page, &self.index)?;
        geo.cells.iter().find(|c| c.chain == chain).map(|c| c.rect[0] + 1.0)
    }

    fn clipped_at(&mut self, p: Position) -> Option<(Hidden, usize, usize, std::rc::Rc<[super::geometry::CellKey]>)> {
        let (para, unit) = p.para_offset()?;
        let n = self.pages.len();
        let near = self.near;
        let order = (0..n).map(|d| if d % 2 == 0 { near + d / 2 } else { near.wrapping_sub(d / 2 + 1) }).filter(|&i| i < n);
        for page in order {
            let Some(geo) = self.pages.page(page, &self.index) else { continue };
            for c in &geo.cells {
                if let Some(k) = c.hidden.iter().position(|&(q, a, b)| q == para && unit >= a && unit <= b) {
                    return Some((c.hidden.clone(), k, page, c.chain.clone()));
                }
            }
        }
        None
    }

    pub(super) fn clipped_vertical(&mut self, focus: Position, x: f64, down: bool) -> Option<Position> {
        let at = |para, unit| Position::text(focus.story, para, unit);
        if let Some((hidden, k, page, chain)) = self.clipped_at(focus) {
            let geo = self.pages.page(page, &self.index)?;
            let next = if down { hidden[k + 1..].iter().find(|h| h.0 != hidden[k].0) } else { hidden[..k].iter().rev().find(|h| h.0 != hidden[k].0) };
            if let Some(&(para, a, _)) = next {
                return Some(at(para, a));
            }
            let last = geo.lines.iter().filter(|l| l.chain == chain && !l.repeat).max_by(|a, b| a.top.total_cmp(&b.top))?;
            let s = last.nearest(x)?;
            let from = at(s.para, s.unit);
            if !down {
                return Some(from);
            }
            let bottom = geo.cells.iter().find(|c| c.chain == chain).map_or(last.bottom, |c| c.rect[3]);
            let dx = |l: &super::geometry::GLine| (l.x0() - x).max(x - l.x1()).max(0.0);
            let below = geo
                .lines
                .iter()
                .filter(|l| !l.repeat && geo.stories[l.story].story == focus.story && l.top >= bottom - 0.01)
                .min_by(|a, b| a.top.total_cmp(&b.top).then(dx(a).total_cmp(&dx(b))));
            return Some(match below.and_then(|l| l.nearest(x)).map(|s| at(s.para, s.unit)) {
                Some(p) => p,
                None => from,
            });
        }
        let line = self.nav().line_of(focus)?;
        let l = &line.geo.lines[line.line];
        if !down {
            let story = line.geo.stories[l.story].story;
            let cell = line
                .geo
                .cells
                .iter()
                .filter(|c| !c.hidden.is_empty() && c.rect[0] <= x && x < c.rect[2] && c.rect[3] <= l.top + 0.5)
                .max_by(|a, b| a.rect[3].total_cmp(&b.rect[3]))?;
            let between = line.geo.lines.iter().any(|m| {
                line.geo.stories[m.story].story == story
                    && m.top >= cell.rect[3] - 0.01
                    && m.bottom <= l.top + 0.01
                    && m.x0() <= x
                    && x <= m.x1().max(m.x0() + 1.0)
            });
            if between || l.chain == cell.chain {
                return None;
            }
            let last_para = cell.hidden.last()?.0;
            let &(para, a, _) = cell.hidden.iter().find(|h| h.0 == last_para)?;
            return Some(at(para, a));
        }
        let cell: &GCell = line.geo.cells.iter().find(|c| c.chain == l.chain && !c.hidden.is_empty())?;
        let below = line.geo.lines.iter().any(|m| m.chain == l.chain && m.top > l.top + 0.01);
        if below {
            return None;
        }
        let &(para, a, _) = cell.hidden.first()?;
        Some(at(para, a))
    }
}
