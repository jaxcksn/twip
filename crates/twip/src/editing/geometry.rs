//! Page geometry

use std::collections::HashSet;
use std::rc::Rc;

use crate::docx::VMerge;
use crate::layout::fragment::{Item, LineFragment, PageFragment};
use crate::layout::traverse::{CellNode, LineNode, SpatialNode, StoryRef, spatial_tree};
use crate::layout::{FloatId, ParaId, TableId, Unit};

use super::index::DocIndex;
use super::position::{Affinity, StoryId};

pub type CellKey = (TableId, usize, usize);

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stop {
    pub x: f64,
    pub para: ParaId,
    pub unit: Unit,
    pub end: bool,
}

#[derive(Debug)]
pub struct GLine {
    pub story: usize,
    pub chain: Rc<[CellKey]>,
    pub paras: Rc<[ParaId]>,
    pub top: f64,
    pub bottom: f64,
    pub pictures: Vec<(ParaId, Unit, [f64; 4])>,
    pub stops: Rc<[Stop]>,
    pub repeat: bool,
}

impl GLine {
    pub fn shows(&self, para: ParaId) -> bool {
        self.paras.contains(&para)
    }

    pub fn para(&self) -> Option<ParaId> {
        self.paras.first().copied()
    }

    pub fn first_of(&self, para: ParaId) -> Option<Unit> {
        self.stops.iter().find(|s| s.para == para).map(|s| s.unit)
    }

    pub fn last_of(&self, para: ParaId) -> Option<Unit> {
        self.stops.iter().rfind(|s| s.para == para).map(|s| s.unit)
    }

    pub fn span_of(&self, para: ParaId) -> Option<(f64, f64)> {
        Some((self.stops.iter().find(|s| s.para == para)?.x, self.stops.iter().rfind(|s| s.para == para)?.x))
    }

    pub fn x0(&self) -> f64 {
        self.stops.first().map_or(0.0, |s| s.x)
    }

    pub fn x1(&self) -> f64 {
        self.stops.last().map_or(0.0, |s| s.x)
    }

    pub fn nearest(&self, x: f64) -> Option<Stop> {
        let (k, s) = self.stops.iter().enumerate().min_by(|a, b| (a.1.x - x).abs().total_cmp(&(b.1.x - x).abs()))?;
        let same = self.stops[k..].iter().take_while(|t| (t.x - s.x).abs() < 0.01);
        Some(*if x > s.x { same.filter(|t| t.para != s.para).last().unwrap_or(s) } else { s })
    }

    pub fn x_of(&self, para: ParaId, u: Unit) -> Option<f64> {
        self.stops.iter().filter(|s| s.para == para && s.unit <= u).max_by_key(|s| s.unit).map(|s| s.x)
    }
}

struct Part {
    para: ParaId,
    stops: Vec<Stop>,
    end: Option<(f64, Unit)>,
    empty_at: Option<f64>,
    label_end: Option<f64>,
}

#[derive(Debug)]
pub struct GCell {
    pub story: usize,
    pub chain: Rc<[CellKey]>,
    pub rect: [f64; 4],
    pub col: usize,
    pub span: usize,
    pub v_merge: VMerge,
    pub clip: Option<f64>,
    pub hidden: Vec<(ParaId, Unit, Unit)>,
    pub closing: Option<(ParaId, [f64; 3])>,
}

#[derive(Debug)]
pub struct GRow {
    pub story: usize,
    pub outer: Rc<[CellKey]>,
    pub table: TableId,
    pub row: usize,
    pub rect: [f64; 4],
    pub top: f64,
    pub repeat: bool,
    pub edges: Vec<(f64, usize)>,
}

#[derive(Debug)]
pub struct GStory {
    pub story: StoryId,
    pub host: StoryId,
    pub float: Option<FloatId>,
    pub rect: [f64; 4],
}

#[derive(Debug, Default)]
pub struct PageGeometry {
    pub w: f64,
    pub h: f64,
    pub body_area: Option<(f64, f64)>,
    pub stories: Vec<GStory>,
    pub lines: Vec<GLine>,
    pub cells: Vec<GCell>,
    pub rows: Vec<GRow>,
    paras: HashSet<ParaId>,
    tables: HashSet<TableId>,
    after_break: std::collections::HashMap<ParaId, Unit>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    Cell(TableId, usize, usize),
    Row(TableId, usize),
    Column(TableId, usize, usize),
    Table(TableId),
    InsertColumn(TableId, usize),
    InsertRow(TableId, usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextHit {
    pub story: StoryId,
    pub para: ParaId,
    pub unit: Unit,
    pub affinity: Affinity,
    pub repeat: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaretRect {
    pub x: f64,
    pub top: f64,
    pub bottom: f64,
}

impl PageGeometry {
    pub fn build(page: &PageFragment, idx: &DocIndex, shows: Option<u32>, memo: &mut LineMemo) -> Option<PageGeometry> {
        let tree = spatial_tree(page, false)?;
        let mut g = PageGeometry { w: page.w, h: page.h, body_area: page.body_area, ..PageGeometry::default() };
        for s in &tree.stories {
            let (story, float) = match s.story {
                StoryRef::Body => (StoryId::Body, None),
                StoryRef::Header(name) | StoryRef::Footer(name) => match idx.header_footer(name) {
                    Some(id) => (id, None),
                    None => continue,
                },
                StoryRef::Note { kind, id } => match kind {
                    crate::docx::NoteKind::Foot => (StoryId::Footnote(id), None),
                    crate::docx::NoteKind::End => (StoryId::Endnote(id), None),
                },
                StoryRef::Float { float, .. } => (StoryId::TextBox(idx.box_story(float)), Some(float)),
            };
            let si = g.stories.len();
            g.stories.push(GStory { story, host: idx.host(story), float, rect: s.rect });
            g.nodes(&s.nodes, si, &Rc::from([]), false, memo);
        }
        let viewed = |s: StoryId| matches!(s, StoryId::BuildingBlock(k) if Some(k) != shows);
        let table_viewed = |t: TableId| idx.table_slot(t).is_some_and(|s| viewed(idx.story(s.container)));
        g.lines.retain(|l| !l.para().and_then(|p| idx.para_slot(p)).is_some_and(|s| viewed(idx.story(s.container))));
        g.rows.retain(|r| !table_viewed(r.table) && !r.outer.iter().any(|k| table_viewed(k.0)));
        g.cells.retain(|c| !c.chain.iter().any(|k| table_viewed(k.0)));
        for i in 0..g.lines.len() {
            let si = g.lines[i].story;
            if g.stories[si].story != StoryId::Body || !g.lines[i].chain.is_empty() {
                continue;
            }
            let Some(slot) = g.lines[i].para().and_then(|p| idx.para_slot(p)) else { continue };
            let s = idx.story(slot.container);
            if s == StoryId::Body {
                continue;
            }
            let (top, bottom) = (g.lines[i].top, g.lines[i].bottom);
            let k = match g.stories.iter().position(|t| t.story == s) {
                Some(k) => {
                    let r = &mut g.stories[k].rect;
                    r[1] = r[1].min(top);
                    r[3] = r[3].max(bottom);
                    k
                }
                None => {
                    let r = g.stories[si].rect;
                    g.stories.push(GStory { story: s, host: s, float: None, rect: [r[0], top, r[2], bottom] });
                    g.stories.len() - 1
                }
            };
            g.lines[i].story = k;
        }
        g.split_bodies();
        Some(g)
    }

    fn split_bodies(&mut self) {
        let bodies: Vec<usize> =
            (0..self.stories.len()).filter(|&i| self.stories[i].story == StoryId::Body && self.stories[i].float.is_none()).collect();
        if bodies.len() < 2 {
            return;
        }
        let extent = |i: usize| {
            let lines = self.lines.iter().filter(|l| l.story == i).map(|l| (l.top, l.bottom));
            let rows = self.rows.iter().filter(|r| r.story == i).map(|r| (r.rect[1], r.rect[3]));
            lines.chain(rows).reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
        };
        let mut bands: Vec<(usize, f64, f64)> = bodies.iter().filter_map(|&i| extent(i).map(|(t, b)| (i, t, b))).collect();
        bands.sort_by(|a, b| a.1.total_cmp(&b.1));
        let (page_top, page_bottom) = (self.stories[bodies[0]].rect[1], self.stories[bodies[0]].rect[3]);
        for &i in &bodies {
            let r = &mut self.stories[i].rect;
            r[3] = r[1];
        }
        for (k, &(i, top, bottom)) in bands.iter().enumerate() {
            let from = if k == 0 { page_top } else { (bands[k - 1].2 + top) / 2.0 };
            let to = bands.get(k + 1).map_or(page_bottom, |next| (bottom + next.1) / 2.0);
            let r = &mut self.stories[i].rect;
            (r[1], r[3]) = (from, to);
        }
    }

    fn nodes(&mut self, nodes: &[SpatialNode], story: usize, chain: &Rc<[CellKey]>, repeat: bool, memo: &mut LineMemo) {
        for n in nodes {
            match n {
                SpatialNode::Line(l) => self.line(l, story, chain, repeat, memo),
                SpatialNode::Table(t) => {
                    self.tables.insert(t.table);
                    for r in &t.rows {
                        self.rows.push(GRow {
                            story,
                            outer: chain.clone(),
                            table: t.table,
                            row: r.row,
                            rect: r.rect,
                            top: r.top,
                            repeat: r.repeat,
                            edges: r.edges.clone(),
                        });
                        for c in &r.cells {
                            self.cell(c, story, chain, t.table, r.row, repeat || r.repeat, memo);
                        }
                    }
                }
            }
        }
    }

    fn cell(&mut self, c: &CellNode, story: usize, outer: &Rc<[CellKey]>, table: TableId, row: usize, repeat: bool, memo: &mut LineMemo) {
        let chain: Rc<[CellKey]> = outer.iter().copied().chain(std::iter::once((table, row, c.place.index))).collect();
        if let Some((p, _)) = c.closing {
            self.paras.insert(p);
        }
        self.cells.push(GCell {
            story,
            chain: chain.clone(),
            rect: c.rect,
            col: c.place.col,
            span: c.place.span,
            v_merge: c.place.v_merge,
            clip: c.clip,
            hidden: c.place.hidden.to_vec(),
            closing: c.closing,
        });
        self.nodes(&c.nodes, story, &chain, repeat, memo);
    }

    fn line(&mut self, l: &LineNode, story: usize, chain: &Rc<[CellKey]>, repeat: bool, memo: &mut LineMemo) {
        let Some(own) = l.para else { return };
        let key = (Rc::as_ptr(l.line), l.x.to_bits(), own);
        let (paras, stops) = match memo.0.get(&key) {
            Some(m) => {
                self.after_break.extend(m.breaks.iter().copied());
                (m.paras.clone(), m.stops.clone())
            }
            None => {
                let Some((paras, stops, breaks, pure)) = self.stops_of(l, own) else { return };
                let (paras, stops): (Rc<[ParaId]>, Rc<[Stop]>) = (paras.into(), stops.into());
                if pure {
                    memo.0.insert(key, Memo { pinned: l.line.clone(), paras: paras.clone(), stops: stops.clone(), breaks });
                }
                (paras, stops)
            }
        };
        if !repeat {
            self.paras.extend(paras.iter().copied());
        }
        let pictures = l
            .line
            .items
            .iter()
            .filter_map(|it| match it {
                Item::Image { rect, .. } => Some(*rect),
                Item::Shape(s) => Some(s.rect),
                _ => None,
            })
            .filter_map(|r| {
                // The picture's atom holds its drawing, inset by any effect extent (an outline's half width).
                let a = l.line.atoms.iter().find(|a| !a.brk && a.x0 - 0.01 <= r[0] && r[0] < a.x1)?;
                Some((a.para, a.unit, [l.x + r[0], l.y + r[1], l.x + r[2], l.y + r[3]]))
            })
            .collect();
        self.lines.push(GLine { story, chain: chain.clone(), paras, top: l.y, bottom: l.y + l.line.pitch, pictures, stops, repeat });
    }

    /// The line's paragraphs and stops, the paragraph ends its breaks leave, and whether all of it depends on the
    /// line alone (no end taken from the lines before it).
    fn stops_of(&mut self, l: &LineNode, own: ParaId) -> Option<(Vec<ParaId>, Vec<Stop>, Box<[(ParaId, Unit)]>, bool)> {
        let mut parts: Vec<Part> = Vec::new();
        fn part(parts: &mut Vec<Part>, para: ParaId) -> &mut Part {
            let k = match parts.iter().position(|p| p.para == para) {
                Some(k) => k,
                None => {
                    parts.push(Part { para, stops: Vec::new(), end: None, empty_at: None, label_end: None });
                    parts.len() - 1
                }
            };
            &mut parts[k]
        }
        for it in l.line.items.iter() {
            let Item::Glyphs(r) = it else { continue };
            let Part { stops, end, empty_at, label_end, para } = part(&mut parts, r.para.unwrap_or(own));
            let para = *para;
            if r.chars.is_empty() {
                empty_at.get_or_insert(l.x + r.x);
                continue;
            }
            let scale = r.face.scale(r.size);
            let end_x = r.glyphs.last().map_or(r.chars[r.chars.len() - 1].1, |&(g, x)| x + r.face.advance(g) * scale);
            let mut xs: Vec<f64> = r.chars.iter().map(|c| c.1).collect();
            let mut k = 0;
            while k < xs.len() {
                let mut e = k + 1;
                while e < xs.len() && xs[e] == xs[k] {
                    e += 1;
                }
                let right = xs.get(e).copied().unwrap_or(end_x);
                for j in k + 1..e {
                    xs[j] = xs[k] + (right - xs[k]) * (j - k) as f64 / (e - k) as f64;
                }
                k = e;
            }
            if r.chars.iter().all(|c| c.2.is_none()) {
                *label_end = Some(label_end.map_or(l.x + end_x, |e: f64| e.max(l.x + end_x)));
            }
            stops.reserve(r.chars.len());
            for (k, c) in r.chars.iter().enumerate() {
                if let Some(u) = c.2 {
                    stops.push(Stop { x: l.x + xs[k], para, unit: u, end: false });
                    if end.is_none_or(|(_, eu)| u.0 + 1 > eu.0) {
                        *end = Some((l.x + end_x, Unit(u.0 + 1)));
                    }
                }
            }
        }
        let mut breaks: Vec<(ParaId, Unit)> = Vec::new();
        for a in l.line.atoms.iter().filter(|a| a.brk) {
            match breaks.iter_mut().find(|b| b.0 == a.para) {
                Some(b) => b.1 = b.1.max(a.unit),
                None => breaks.push((a.para, a.unit)),
            }
        }
        let breaks: Box<[(ParaId, Unit)]> = breaks.into_iter().map(|(p, u)| (p, Unit(u.0 + 1))).collect();
        self.after_break.extend(breaks.iter().copied());
        for a in l.line.atoms.iter() {
            let Part { stops, end, .. } = part(&mut parts, a.para);
            let after = if a.brk { a.unit } else { Unit(a.unit.0 + 1) };
            if !a.brk {
                stops.push(Stop { x: l.x + a.x0, para: a.para, unit: a.unit, end: false });
            }
            if end.is_none_or(|(_, eu)| after.0 > eu.0 || (a.brk && after.0 >= eu.0)) {
                *end = Some((l.x + if a.brk { a.x0 } else { a.x1 }, after));
            }
        }
        let mut done: Vec<(ParaId, Vec<Stop>)> = Vec::new();
        let mut pure = true;
        for p in parts {
            let mut own = p.stops;
            if let Some((_, eu)) = p.end {
                own.retain(|s| s.unit != eu);
            }
            match p.end {
                Some((x, u)) => own.push(Stop { x, para: p.para, unit: u, end: true }),
                None => match p.empty_at.or(p.label_end) {
                    Some(x) => {
                        let unit = l.line.mark.filter(|_| Some(p.para) == l.para).unwrap_or_else(|| {
                            pure = false;
                            self.after_break
                                .get(&p.para)
                                .copied()
                                .unwrap_or_else(|| self.lines.iter().filter_map(|m| m.last_of(p.para)).max().unwrap_or(Unit(0)))
                        });
                        own.push(Stop { x, para: p.para, unit, end: true })
                    }
                    None => continue,
                },
            }
            done.push((p.para, own));
        }
        if done.is_empty() {
            return None;
        }
        let left = |s: &[Stop]| s.iter().map(|s| s.x).fold(f64::INFINITY, f64::min);
        done.sort_by(|a, b| left(&a.1).total_cmp(&left(&b.1)));
        let paras: Vec<ParaId> = done.iter().map(|d| d.0).collect();
        let mut stops: Vec<Stop> = done
            .into_iter()
            .map(|d| d.1)
            .reduce(|mut a, b| {
                a.extend(b);
                a
            })
            .unwrap_or_default();
        let order = |q: ParaId| paras.iter().position(|&p| p == q);
        stops.sort_by(|a, b| a.x.total_cmp(&b.x).then(order(a.para).cmp(&order(b.para))).then(a.unit.cmp(&b.unit)));
        stops.dedup_by(|b, a| a.para == b.para && a.unit == b.unit && !b.end);
        Some((paras, stops, breaks, pure))
    }

    pub fn shows_para(&self, p: ParaId) -> bool {
        self.paras.contains(&p)
    }

    pub fn shows_table(&self, t: TableId) -> bool {
        self.tables.contains(&t)
    }

    pub fn lines_of(&self, para: ParaId) -> impl Iterator<Item = (usize, &GLine)> {
        self.lines.iter().enumerate().filter(move |(_, l)| l.shows(para) && !l.repeat)
    }

    pub fn line_at(&self, para: ParaId, u: Unit, affinity: Affinity) -> Option<usize> {
        let first = |l: &GLine| l.first_of(para).unwrap_or_default();
        let last = |l: &GLine| l.last_of(para).unwrap_or_default();
        let mut found: Vec<usize> = self.lines_of(para).filter(|(_, l)| first(l) <= u && u <= last(l)).map(|(i, _)| i).collect();
        if found.is_empty() {
            return self.lines_of(para).filter(|(_, l)| first(l) <= u).map(|(i, _)| i).last();
        }
        if found.len() > 1 {
            let i = match affinity {
                Affinity::Upstream => *found.iter().find(|&&i| last(&self.lines[i]) == u).unwrap_or(&found[0]),
                Affinity::Downstream => *found.iter().rev().find(|&&i| first(&self.lines[i]) == u).unwrap_or(&found[found.len() - 1]),
            };
            found = vec![i];
        }
        found.first().copied()
    }

    pub fn caret(&self, para: ParaId, u: Unit, affinity: Affinity) -> Option<CaretRect> {
        if let Some(i) = self.line_at(para, u, affinity) {
            let l = &self.lines[i];
            let x = l.x_of(para, u)?;
            return Some(CaretRect { x, top: l.top, bottom: l.bottom });
        }
        self.cells.iter().find_map(|c| match c.closing {
            Some((p, [x, top, h])) if p == para => Some(CaretRect { x, top, bottom: top + h }),
            _ => None,
        })
    }

    fn story_at(&self, x: f64, y: f64, want: &dyn Fn(StoryId) -> bool) -> Option<usize> {
        let inside = |r: &[f64; 4]| x >= r[0] && x <= r[2] && y >= r[1] && y <= r[3];
        let float = self.stories.iter().enumerate().rev().find(|(_, s)| s.float.is_some() && want(s.host) && inside(&s.rect) && self.has_lines(s));
        if let Some((i, _)) = float {
            return Some(i);
        }
        let candidates: Vec<usize> = (0..self.stories.len()).filter(|&i| self.stories[i].float.is_none() && want(self.stories[i].story)).collect();
        let area = |r: &[f64; 4]| (r[2] - r[0]) * (r[3] - r[1]);
        candidates
            .iter()
            .copied()
            .filter(|&i| inside(&self.stories[i].rect))
            .min_by(|&a, &b| area(&self.stories[a].rect).total_cmp(&area(&self.stories[b].rect)))
            .or_else(|| candidates.iter().copied().min_by(|&a, &b| dist(&self.stories[a].rect, x, y).total_cmp(&dist(&self.stories[b].rect, x, y))))
    }

    fn has_lines(&self, s: &GStory) -> bool {
        let si = self.stories.iter().position(|t| std::ptr::eq(t, s));
        self.lines.iter().any(|l| Some(l.story) == si)
    }

    pub fn cell_at(&self, story: usize, x: f64, y: f64) -> Option<&GCell> {
        self.cells
            .iter()
            .filter(|c| c.story == story && x >= c.rect[0] && x < c.rect[2] && y >= c.rect[1] && y < c.rect[3])
            .max_by_key(|c| c.chain.len())
    }

    pub fn zone(&self, x: f64, y: f64, px: f64, hovered: Option<TableId>) -> Option<Zone> {
        let deepest = |v: &mut Vec<(usize, Zone)>| v.iter().max_by_key(|(d, _)| *d).map(|(_, z)| *z);
        let row_left = |r: &GRow| {
            self.cells
                .iter()
                .filter(|c| c.chain.last().is_some_and(|k| k.0 == r.table && k.1 == r.row))
                .map(|c| c.rect[0])
                .fold(f64::INFINITY, f64::min)
        };
        let mut found: Vec<(usize, Zone)> = Vec::new();
        for c in &self.cells {
            let Some(&(t, row, cell)) = c.chain.last() else { continue };
            let [x0, y0, x1, y1] = c.rect;
            if y < y0 || y >= y1 {
                continue;
            }
            if x >= x0 + 2.0 * px && x < x0 + 8.0 * px {
                found.push((c.chain.len(), Zone::Cell(t, row, cell)));
            } else if row == 0 && x >= x0 && x < x1 && y < y0 + 4.0 * px {
                found.push((c.chain.len(), Zone::Column(t, row, cell)));
            }
        }
        if let Some(z) = deepest(&mut found) {
            return Some(z);
        }
        for r in self.rows.iter().filter(|r| r.row == 0 && !r.repeat) {
            let [x0, y0, ..] = r.rect;
            if (x0 - 20.0 * px..=x0 - 5.0 * px).contains(&x) && (y0 - 20.0 * px..=y0 - 5.0 * px).contains(&y) {
                found.push((r.outer.len(), Zone::Table(r.table)));
            }
        }
        if let Some(z) = deepest(&mut found) {
            return Some(z);
        }
        for r in self.rows.iter().filter(|r| Some(r.table) == hovered && !r.repeat) {
            let [_, y0, _, y1] = r.rect;
            if r.row == 0
                && y >= y0 - 22.0 * px
                && y < y0
                && let Some(&(_, k)) = r.edges.iter().find(|(ex, _)| (x - ex).abs() <= 7.0 * px)
            {
                return Some(Zone::InsertColumn(r.table, k));
            }
            let left = row_left(r);
            if x >= left - 24.0 * px && x < left && (y - y1).abs() <= 7.0 * px {
                return Some(Zone::InsertRow(r.table, r.row + 1));
            }
        }
        for r in &self.rows {
            if y < r.rect[1] || y >= r.rect[3] {
                continue;
            }
            let left = row_left(r);
            if !left.is_finite() || x > left - 3.0 * px {
                continue;
            }
            let outer_left = r.outer.last().and_then(|k| self.cells.iter().find(|c| c.chain.last() == Some(k))).map_or(0.0, |c| c.rect[0]);
            if x >= outer_left {
                found.push((r.outer.len(), Zone::Row(r.table, r.row)));
            }
        }
        deepest(&mut found)
    }

    pub fn selection_bar(&self, x: f64, y: f64, left: f64) -> Option<usize> {
        let si = self.stories.iter().position(|s| s.story == StoryId::Body && s.float.is_none() && y >= s.rect[1] && y <= s.rect[3])?;
        if x < 0.0 || x > left {
            return None;
        }
        if self.rows.iter().any(|r| r.story == si && r.outer.is_empty() && y >= r.rect[1] && y < r.rect[3]) {
            return None;
        }
        let vdist = |l: &GLine| {
            if y < l.top {
                l.top - y
            } else if y > l.bottom {
                y - l.bottom
            } else {
                0.0
            }
        };
        (0..self.lines.len())
            .filter(|&i| self.lines[i].story == si && self.lines[i].chain.is_empty())
            .min_by(|&a, &b| vdist(&self.lines[a]).total_cmp(&vdist(&self.lines[b])))
    }

    pub fn hit_text(&self, x: f64, y: f64, want: &dyn Fn(StoryId) -> bool) -> Option<TextHit> {
        let si = self.story_at(x, y, want)?;
        let story = self.stories[si].story;
        let mut cell = self.cell_at(si, x, y);
        if let Some(c) = cell
            && c.v_merge == VMerge::Continue
            && let Some(&(t, row, _)) = c.chain.last()
        {
            let outer = &c.chain[..c.chain.len() - 1];
            cell = self
                .cells
                .iter()
                .filter(|s| s.story == si && s.col == c.col && s.v_merge == VMerge::Restart && &s.chain[..s.chain.len() - 1] == outer)
                .filter(|s| s.chain.last().is_some_and(|&(st, sr, _)| st == t && sr < row))
                .max_by_key(|s| s.chain.last().map(|k| k.1))
                .or(cell);
        }
        let chain: &[CellKey] = cell.map_or(&[], |c| &c.chain);
        let candidates = self.lines.iter().filter(|l| l.story == si && &*l.chain == chain);
        let best = candidates.min_by(|a, b| line_dist(a, x, y).total_cmp(&line_dist(b, x, y)));
        if let Some(c) = cell
            && let Some((p, [_, top, _])) = c.closing
            && best.is_none_or(|l| y > l.bottom && top <= y + 1.0)
        {
            return Some(TextHit { story, para: p, unit: Unit(0), affinity: Affinity::Downstream, repeat: false });
        }
        let line = match best {
            Some(l) => l,
            None => self
                .lines
                .iter()
                .filter(|l| l.story == si && l.chain.starts_with(chain))
                .min_by(|a, b| line_dist(a, x, y).total_cmp(&line_dist(b, x, y)))?,
        };
        let stop = line.nearest(x)?;
        let wraps = stop.end && self.lines.iter().any(|m| m.first_of(stop.para) == Some(stop.unit) && !std::ptr::eq(m, line));
        Some(TextHit {
            story,
            para: stop.para,
            unit: stop.unit,
            affinity: if wraps { Affinity::Upstream } else { Affinity::Downstream },
            repeat: line.repeat,
        })
    }
}

fn dist(r: &[f64; 4], x: f64, y: f64) -> f64 {
    let dx = if x < r[0] {
        r[0] - x
    } else if x > r[2] {
        x - r[2]
    } else {
        0.0
    };
    let dy = if y < r[1] {
        r[1] - y
    } else if y > r[3] {
        y - r[3]
    } else {
        0.0
    };
    dx + dy
}

fn line_dist(l: &GLine, x: f64, y: f64) -> f64 {
    let dy = if y < l.top {
        l.top - y
    } else if y > l.bottom {
        y - l.bottom
    } else {
        0.0
    };
    let dx = if x < l.x0() {
        l.x0() - x
    } else if x > l.x1() {
        x - l.x1()
    } else {
        0.0
    };
    dy * 1000.0 + dx
}

#[derive(Default)]
pub struct Pages {
    snapshot: Option<Rc<crate::layout::Snapshot>>,
    pub shows: Option<u32>,
    by_page: Vec<Option<Rc<PageGeometry>>>,
    cache: std::collections::HashMap<crate::layout::FragmentId, Rc<PageGeometry>>,
    lines: LineMemo,
}

// Line memo
#[derive(Default)]
pub struct LineMemo(std::collections::HashMap<(*const LineFragment, u64, ParaId), Memo>);

struct Memo {
    pinned: Rc<LineFragment>,
    paras: Rc<[ParaId]>,
    stops: Rc<[Stop]>,
    breaks: Box<[(ParaId, Unit)]>,
}

impl Pages {
    pub fn sync(&mut self, snapshot: &Rc<crate::layout::Snapshot>) {
        if self.snapshot.as_ref().is_some_and(|s| Rc::ptr_eq(s, snapshot)) {
            return;
        }
        let ids: HashSet<crate::layout::FragmentId> = snapshot.pages().iter().map(|p| p.id).collect();
        self.cache.retain(|id, _| ids.contains(id));
        self.by_page = vec![None; snapshot.pages().len()];
        self.snapshot = Some(snapshot.clone());
        self.lines.0.retain(|_, m| Rc::strong_count(&m.pinned) > 1);
    }

    #[cfg(test)]
    pub fn assert_memo_fresh(&mut self, idx: &DocIndex) {
        let Some(snapshot) = self.snapshot.clone() else { return };
        for i in (0..self.by_page.len()).filter(|&i| self.by_page[i].is_some()) {
            let Some((frag, _)) = snapshot.page(i) else { continue };
            let warm = PageGeometry::build(frag, idx, self.shows, &mut self.lines).map(|g| format!("{:?}", g.lines));
            let cold = PageGeometry::build(frag, idx, self.shows, &mut LineMemo::default()).map(|g| format!("{:?}", g.lines));
            assert_eq!(warm, cold, "page {i}: the line memo differs from a fresh build");
        }
    }

    pub fn snapshot(&self) -> Option<&Rc<crate::layout::Snapshot>> {
        self.snapshot.as_ref()
    }

    pub fn len(&self) -> usize {
        self.by_page.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_page.is_empty()
    }

    pub fn page(&mut self, i: usize, idx: &DocIndex) -> Option<Rc<PageGeometry>> {
        if let Some(Some(g)) = self.by_page.get(i) {
            return Some(g.clone());
        }
        let snapshot = self.snapshot.clone()?;
        let (frag, info) = snapshot.page(i)?;
        if info.provisional {
            return None;
        }
        let g = match self.cache.get(&frag.id) {
            Some(g) => g.clone(),
            None => {
                let g = Rc::new(PageGeometry::build(frag, idx, self.shows, &mut self.lines)?);
                self.cache.insert(frag.id, g.clone());
                g
            }
        };
        self.by_page[i] = Some(g.clone());
        Some(g)
    }

    pub fn line_showing(
        &mut self,
        para: ParaId,
        u: Unit,
        affinity: Affinity,
        near: usize,
        idx: &DocIndex,
    ) -> Option<(usize, Rc<PageGeometry>, usize)> {
        let pages = self.pages_of(para, near, idx);
        let mut before = None;
        for (k, &page) in pages.iter().enumerate() {
            let Some(geo) = self.page(page, idx) else { continue };
            let Some(i) = geo.line_at(para, u, affinity) else { continue };
            let l = &geo.lines[i];
            if l.last_of(para).is_some_and(|e| e < u) {
                before = Some((page, geo, i));
                continue;
            }
            if affinity == Affinity::Downstream
                && l.last_of(para) == Some(u)
                && l.first_of(para) != Some(u)
                && let Some(&next) = pages.get(k + 1)
                && let Some(g2) = self.page(next, idx)
            {
                let j = g2.lines_of(para).find(|(_, m)| m.first_of(para) == Some(u)).map(|(j, _)| j);
                if let Some(j) = j {
                    return Some((next, g2, j));
                }
            }
            return Some((page, geo, i));
        }
        before
    }

    pub fn pages_of(&mut self, para: ParaId, near: usize, idx: &DocIndex) -> Vec<usize> {
        let n = self.len();
        let near = near.min(n.saturating_sub(1));
        let mut out = Vec::new();
        for d in 0..n {
            for i in [near.checked_sub(d), (d > 0).then_some(near + d)].into_iter().flatten() {
                if i < n && self.page(i, idx).is_some_and(|g| g.shows_para(para)) {
                    out.push(i);
                }
            }
            if !out.is_empty() && d > 0 {
                let lo = *out.iter().min().unwrap_or(&0);
                let hi = *out.iter().max().unwrap_or(&0);
                let below_done = lo == 0 || !self.page(lo - 1, idx).is_some_and(|g| g.shows_para(para));
                let above_done = hi + 1 >= n || !self.page(hi + 1, idx).is_some_and(|g| g.shows_para(para));
                if below_done && above_done {
                    break;
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }
}
