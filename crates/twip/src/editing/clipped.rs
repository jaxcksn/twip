//! Positions without geometry: text an exact row height clips. The keyboard reaches it (Up and Down
//! go into and through it, Left and Right step through it as anywhere), the caret isn't drawn there and the view
//! doesn't scroll to it, and clicks never resolve into it (it has no lines).

use crate::layout::Unit;

use super::Editor;
use super::geometry::GCell;
use super::position::Position;

/// The clipped units of a cell: (paragraph, first unit, end unit) in document order.
type Hidden = Vec<(crate::layout::ParaId, Unit, Unit)>;

impl Editor {
    /// Why the caret isn't drawn, when it isn't: `"clipped"` inside text an exact row height hides.
    pub fn caret_hidden(&mut self) -> Option<&'static str> {
        let f = self.selection.focus()?;
        // Inside the clipped part (its start is also the drawn line's end when a paragraph is clipped part-way).
        let inside = self.clipped_at(f).is_some_and(|(h, k, ..)| f.para_offset().is_some_and(|(_, u)| u > h[k].1 || h[k].1 == Unit(0)));
        if !inside && self.caret_view().is_some() {
            return None;
        }
        self.clipped_at(f).map(|_| "clipped")
    }

    /// Home / End in clipped text (no lines to take the edges from): the clipped part's start or end in its paragraph.
    pub(super) fn clipped_line_edge(&mut self, p: Position, end: bool) -> Option<Position> {
        let (hidden, k, ..) = self.clipped_at(p)?;
        let (para, a, b) = hidden[k];
        Some(Position::text(p.story, para, if end { b } else { a }))
    }

    /// A clipped position's x for vertical moves: its cell's left edge (it has no line to measure on).
    pub(super) fn clipped_x(&mut self, p: Position) -> Option<f64> {
        let (_, _, page, chain) = self.clipped_at(p)?;
        let geo = self.pages.page(page, &self.index)?;
        geo.cells.iter().find(|c| c.chain == chain).map(|c| c.rect[0] + 1.0)
    }

    /// The clipped units of the cell whose hidden text holds `p`, with `p`'s range among them.
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

    /// Up or Down where clipped text is involved: from a cell's last drawn line down into its clipped text;
    /// within it, to the next or previous clipped paragraph; out of it, on to what is below the cell or up to its
    /// last drawn line. `None`: ordinary vertical movement.
    // shortcut: clipped text has no lines here, so these moves go to a clipped paragraph's start, not to the goal x.
    pub(super) fn clipped_vertical(&mut self, focus: Position, x: f64, down: bool) -> Option<Position> {
        let at = |para, unit| Position::text(focus.story, para, unit);
        if let Some((hidden, k, page, chain)) = self.clipped_at(focus) {
            let geo = self.pages.page(page, &self.index)?;
            // The start of the next or previous clipped paragraph.
            let next = if down { hidden[k + 1..].iter().find(|h| h.0 != hidden[k].0) } else { hidden[..k].iter().rev().find(|h| h.0 != hidden[k].0) };
            if let Some(&(para, a, _)) = next {
                return Some(at(para, a));
            }
            // Out of the clipped text: up to the cell's last drawn line, or down to the nearest drawn line below the
            // cell, at the goal x.
            let last = geo.lines.iter().filter(|l| l.chain == chain && !l.repeat).max_by(|a, b| a.top.total_cmp(&b.top))?;
            let from = at(last.para, last.nearest(x)?.unit);
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
            return Some(match below.and_then(|l| Some(at(l.para, l.nearest(x)?.unit))) {
                Some(p) => p,
                None => from,
            });
        }
        let line = self.nav().line_of(focus)?;
        let l = &line.geo.lines[line.line];
        if !down {
            // Up from the line just below a cell with clipped text (no drawn line between): into its last clipped
            // paragraph.
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
        // Down from a cell's last drawn line: into its clipped text.
        let cell: &GCell = line.geo.cells.iter().find(|c| c.chain == l.chain && !c.hidden.is_empty())?;
        let below = line.geo.lines.iter().any(|m| m.chain == l.chain && m.top > l.top + 0.01);
        if below {
            return None;
        }
        let &(para, a, _) = cell.hidden.first()?;
        Some(at(para, a))
    }
}
