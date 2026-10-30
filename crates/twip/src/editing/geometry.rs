//! Per-page geometry from the layout's spatial tree: hit testing, caret rectangles, selection rectangles and table
//! targets. Built once per page fragment and cached by its `FragmentId`, so a page a new snapshot
//! shares keeps its geometry.
//!
//! Lines and cells carry their container chain, (table, row, model cell) from the story root down: hit testing finds
//! the deepest cell containing the point and only considers lines of exactly that chain, so a click resolves to the
//! nesting level that contains it.

use std::collections::HashSet;
use std::rc::Rc;

use crate::docx::VMerge;
use crate::layout::fragment::{Item, PageFragment};
use crate::layout::traverse::{CellNode, LineNode, SpatialNode, StoryRef, spatial_tree};
use crate::layout::{FloatId, ParaId, TableId, Unit};

use super::index::DocIndex;
use super::position::{Affinity, StoryId};

/// (table, row, model cell index).
pub type CellKey = (TableId, usize, usize);

/// A caret stop: a paragraph's unit boundary's x on a line. `end` marks its paragraph's last stop on the line (after
/// its last character there).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stop {
    pub x: f64,
    pub para: ParaId,
    pub unit: Unit,
    pub end: bool,
}

/// A drawn line. It shows one paragraph, or several where the markup view joins paragraphs (REV-6): its stops say
/// whose units they are.
#[derive(Debug)]
pub struct GLine {
    pub story: usize,
    pub chain: Rc<[CellKey]>,
    /// The paragraphs it shows, in order.
    pub paras: Vec<ParaId>,
    pub top: f64,
    pub bottom: f64,
    /// Its inline pictures as layout draws them: paragraph, unit and box (page coordinates).
    pub pictures: Vec<(ParaId, Unit, [f64; 4])>,
    /// Stops in x order (left to right).
    pub stops: Vec<Stop>,
    /// In a repeated header row's copy: clicks resolve to the original row's positions, and the caret and
    /// navigation use the original.
    pub repeat: bool,
}

impl GLine {
    /// Whether the line shows `para`.
    pub fn shows(&self, para: ParaId) -> bool {
        self.paras.contains(&para)
    }

    /// The first paragraph it shows.
    pub fn para(&self) -> Option<ParaId> {
        self.paras.first().copied()
    }

    /// `para`'s first unit on the line (its leftmost stop).
    pub fn first_of(&self, para: ParaId) -> Option<Unit> {
        self.stops.iter().find(|s| s.para == para).map(|s| s.unit)
    }

    /// `para`'s last unit on the line (its rightmost stop).
    pub fn last_of(&self, para: ParaId) -> Option<Unit> {
        self.stops.iter().rfind(|s| s.para == para).map(|s| s.unit)
    }

    /// Where `para`'s part of the line starts and ends: its leftmost and rightmost stops' x.
    pub fn span_of(&self, para: ParaId) -> Option<(f64, f64)> {
        Some((self.stops.iter().find(|s| s.para == para)?.x, self.stops.iter().rfind(|s| s.para == para)?.x))
    }

    pub fn x0(&self) -> f64 {
        self.stops.first().map_or(0.0, |s| s.x)
    }

    pub fn x1(&self) -> f64 {
        self.stops.last().map_or(0.0, |s| s.x)
    }

    /// The stop nearest `x`. Where two paragraphs meet at one x (REV-6), a point right of it is in the later one.
    pub fn nearest(&self, x: f64) -> Option<Stop> {
        let (k, s) = self.stops.iter().enumerate().min_by(|a, b| (a.1.x - x).abs().total_cmp(&(b.1.x - x).abs()))?;
        let same = self.stops[k..].iter().take_while(|t| (t.x - s.x).abs() < 0.01);
        Some(*if x > s.x { same.filter(|t| t.para != s.para).last().unwrap_or(s) } else { s })
    }

    /// The x of `para`'s unit `u` on this line (the closest stop at or before it).
    pub fn x_of(&self, para: ParaId, u: Unit) -> Option<f64> {
        self.stops.iter().filter(|s| s.para == para && s.unit <= u).max_by_key(|s| s.unit).map(|s| s.x)
    }
}

/// One paragraph's part of a line while its stops are collected: its stops, its end (x, unit), and the caret places
/// of a line where it draws no characters (a run without glyphs, or after a list label).
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
    /// The chain down to and including this cell.
    pub chain: Rc<[CellKey]>,
    pub rect: [f64; 4],
    pub col: usize,
    pub span: usize,
    pub v_merge: VMerge,
    pub clip: Option<f64>,
    pub hidden: Vec<(ParaId, Unit, Unit)>,
    /// A closing paragraph after a nested table: its caret place (x, top, height).
    pub closing: Option<(ParaId, [f64; 3])>,
}

#[derive(Debug)]
pub struct GRow {
    pub story: usize,
    /// The chain of the table's container (the cells enclosing the table).
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
    /// The story whose area shows this one: a float's is the story it is anchored in (the body, a header or footer, a
    /// note); any other story is its own. Whether a click reaches the story goes by it.
    pub host: StoryId,
    /// A floating object's box (a picture or a text box).
    pub float: Option<FloatId>,
    pub rect: [f64; 4],
}

/// What a page shows, for editing.
#[derive(Debug, Default)]
pub struct PageGeometry {
    pub w: f64,
    pub h: f64,
    /// The first section's body top and bottom: above it is the header area, below it the footer's.
    pub body_area: Option<(f64, f64)>,
    pub stories: Vec<GStory>,
    pub lines: Vec<GLine>,
    pub cells: Vec<GCell>,
    pub rows: Vec<GRow>,
    paras: HashSet<ParaId>,
    tables: HashSet<TableId>,
    /// Each paragraph's unit after its last line break so far (the empty line after a trailing break starts there).
    after_break: std::collections::HashMap<ParaId, Unit>,
}

/// A table selection zone under a point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    /// Inside a cell's left edge: selects the cell (table, row, model cell).
    Cell(TableId, usize, usize),
    /// Left of a row, outside its table: selects the row.
    Row(TableId, usize),
    /// Just inside the top of a table's first row: selects the column of that cell.
    Column(TableId, usize, usize),
    /// The move handle just above and left of a table's top left corner: selects the table.
    Table(TableId),
}

/// A text position under a point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextHit {
    pub story: StoryId,
    pub para: ParaId,
    pub unit: Unit,
    pub affinity: Affinity,
    /// In a repeated header row's copy (positions are the original row's).
    pub repeat: bool,
}

/// A caret's place on a page: x, top and bottom.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CaretRect {
    pub x: f64,
    pub top: f64,
    pub bottom: f64,
}

impl PageGeometry {
    pub fn build(page: &PageFragment, idx: &DocIndex) -> Option<PageGeometry> {
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
                StoryRef::Float { float, .. } => (StoryId::TextBox(float), Some(float)),
            };
            let si = g.stories.len();
            g.stories.push(GStory { story, host: idx.host(story), float, rect: s.rect });
            g.nodes(&s.nodes, si, &Rc::from([]), false);
        }
        // Endnotes are laid out in the body's flow: their lines get their own story, boxed by those lines.
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

    /// Several sections' bodies on one page: each body story's box is the band its lines and rows take, split
    /// halfway between neighbouring bands, so a point resolves to the section drawn there. The first band reaches the
    /// page's top and the last its bottom, so a page with one section keeps the whole page. A body with nothing on
    /// the page gets no area.
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

    fn nodes(&mut self, nodes: &[SpatialNode], story: usize, chain: &Rc<[CellKey]>, repeat: bool) {
        for n in nodes {
            match n {
                SpatialNode::Line(l) => self.line(l, story, chain, repeat),
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
                            self.cell(c, story, chain, t.table, r.row, repeat || r.repeat);
                        }
                    }
                }
            }
        }
    }

    fn cell(&mut self, c: &CellNode, story: usize, outer: &Rc<[CellKey]>, table: TableId, row: usize, repeat: bool) {
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
        self.nodes(&c.nodes, story, &chain, repeat);
    }

    fn line(&mut self, l: &LineNode, story: usize, chain: &Rc<[CellKey]>, repeat: bool) {
        let Some(own) = l.para else { return };
        // Each paragraph's part of the line, in the order drawn (one, unless the view joins paragraphs, REV-6).
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
                // An empty line's caret position, or a paragraph's that draws nothing here.
                empty_at.get_or_insert(l.x + r.x);
                continue;
            }
            let scale = r.face.scale(r.size);
            let end_x = r.glyphs.last().map_or(r.chars[r.chars.len() - 1].1, |&(g, x)| x + r.face.advance(g) * scale);
            // Characters of one cluster (ligatures) share an origin: spread them over the cluster's width.
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
                // A list label (no units): an empty item's caret goes after it.
                *label_end = Some(label_end.map_or(l.x + end_x, |e: f64| e.max(l.x + end_x)));
            }
            for (k, c) in r.chars.iter().enumerate() {
                if let Some(u) = c.2 {
                    stops.push(Stop { x: l.x + xs[k], para, unit: u, end: false });
                    if end.is_none_or(|(_, eu)| u.0 + 1 > eu.0) {
                        *end = Some((l.x + end_x, Unit(u.0 + 1)));
                    }
                }
            }
        }
        // Pictures and tabs: a stop before each, and the paragraph's end on the line after the last.
        let mut breaks: Vec<(ParaId, Unit)> = Vec::new();
        for a in l.line.atoms.iter().filter(|a| a.brk) {
            match breaks.iter_mut().find(|b| b.0 == a.para) {
                Some(b) => b.1 = b.1.max(a.unit),
                None => breaks.push((a.para, a.unit)),
            }
        }
        for (p, u) in breaks {
            self.after_break.insert(p, Unit(u.0 + 1));
        }
        for a in l.line.atoms.iter() {
            let Part { stops, end, .. } = part(&mut parts, a.para);
            // A break ends its line: the line's last caret place is before it (End, a click past the line's end).
            let after = if a.brk { a.unit } else { Unit(a.unit.0 + 1) };
            if !a.brk {
                stops.push(Stop { x: l.x + a.x0, para: a.para, unit: a.unit, end: false });
            }
            if end.is_none_or(|(_, eu)| after.0 > eu.0 || (a.brk && after.0 >= eu.0)) {
                *end = Some((l.x + if a.brk { a.x0 } else { a.x1 }, after));
            }
        }
        let mut done: Vec<(ParaId, Vec<Stop>)> = Vec::new();
        for p in parts {
            let mut own = p.stops;
            // A stop for the end unit already there (an atom's) gives way to the end stop.
            if let Some((_, eu)) = p.end {
                own.retain(|s| s.unit != eu);
            }
            match p.end {
                Some((x, u)) => own.push(Stop { x, para: p.para, unit: u, end: true }),
                None => match p.empty_at.or(p.label_end) {
                    // An empty line: the paragraph's start, or after a trailing break its end (the units before it).
                    Some(x) => {
                        let unit = self
                            .after_break
                            .get(&p.para)
                            .copied()
                            .unwrap_or_else(|| self.lines.iter().filter_map(|m| m.last_of(p.para)).max().unwrap_or(Unit(0)));
                        own.push(Stop { x, para: p.para, unit, end: true })
                    }
                    None => continue,
                },
            }
            done.push((p.para, own));
        }
        if done.is_empty() {
            return;
        }
        // Paragraphs side by side (REV-6) in order: by where they start, and where two meet at one x, as drawn.
        let left = |s: &[Stop]| s.iter().map(|s| s.x).fold(f64::INFINITY, f64::min);
        done.sort_by(|a, b| left(&a.1).total_cmp(&left(&b.1)));
        let paras: Vec<ParaId> = done.iter().map(|d| d.0).collect();
        let mut stops: Vec<Stop> = done.into_iter().flat_map(|d| d.1).collect();
        let order = |q: ParaId| paras.iter().position(|&p| p == q);
        stops.sort_by(|a, b| a.x.total_cmp(&b.x).then(order(a.para).cmp(&order(b.para))).then(a.unit.cmp(&b.unit)));
        // A field's drawn value or label is one unit: only its first glyph is a stop, so a click on its far half
        // goes after it (to the next unit's stop).
        stops.dedup_by(|b, a| a.para == b.para && a.unit == b.unit && !b.end);
        if !repeat {
            self.paras.extend(paras.iter().copied());
        }
        // The drawn picture items, each the atom that starts where it does.
        let pictures = l
            .line
            .items
            .iter()
            .filter_map(|it| if let Item::Image { rect, .. } = it { Some(*rect) } else { None })
            .filter_map(|r| {
                let a = l.line.atoms.iter().find(|a| !a.brk && (a.x0 - r[0]).abs() < 0.01)?;
                Some((a.para, a.unit, [l.x + r[0], l.y + r[1], l.x + r[2], l.y + r[3]]))
            })
            .collect();
        self.lines.push(GLine { story, chain: chain.clone(), paras, top: l.y, bottom: l.y + l.line.pitch, pictures, stops, repeat });
    }

    pub fn shows_para(&self, p: ParaId) -> bool {
        self.paras.contains(&p)
    }

    pub fn shows_table(&self, t: TableId) -> bool {
        self.tables.contains(&t)
    }

    /// The lines showing `para`, in flow order (not repeated header copies).
    pub fn lines_of(&self, para: ParaId) -> impl Iterator<Item = (usize, &GLine)> {
        self.lines.iter().enumerate().filter(move |(_, l)| l.shows(para) && !l.repeat)
    }

    /// The line showing unit `u` of `para` with the given affinity: at a wrap the same unit ends one line (upstream)
    /// and starts the next (downstream).
    pub fn line_at(&self, para: ParaId, u: Unit, affinity: Affinity) -> Option<usize> {
        let first = |l: &GLine| l.first_of(para).unwrap_or_default();
        let last = |l: &GLine| l.last_of(para).unwrap_or_default();
        let mut found: Vec<usize> = self.lines_of(para).filter(|(_, l)| first(l) <= u && u <= last(l)).map(|(i, _)| i).collect();
        if found.is_empty() {
            // A unit without a stop of its own (inside a field): the last line starting at or before it.
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

    /// The caret rectangle for unit `u` of `para`.
    pub fn caret(&self, para: ParaId, u: Unit, affinity: Affinity) -> Option<CaretRect> {
        if let Some(i) = self.line_at(para, u, affinity) {
            let l = &self.lines[i];
            let x = l.x_of(para, u)?;
            return Some(CaretRect { x, top: l.top, bottom: l.bottom });
        }
        // A closing paragraph after a nested table.
        self.cells.iter().find_map(|c| match c.closing {
            Some((p, [x, top, h])) if p == para => Some(CaretRect { x, top, bottom: top + h }),
            _ => None,
        })
    }

    /// The story under a point: a floating text box whose host `want` accepts, else the first story whose box contains
    /// it and that `want` accepts (the body, or the header or footer being edited).
    fn story_at(&self, x: f64, y: f64, want: &dyn Fn(StoryId) -> bool) -> Option<usize> {
        let inside = |r: &[f64; 4]| x >= r[0] && x <= r[2] && y >= r[1] && y <= r[3];
        let float = self.stories.iter().enumerate().rev().find(|(_, s)| s.float.is_some() && want(s.host) && inside(&s.rect) && self.has_lines(s));
        if let Some((i, _)) = float {
            return Some(i);
        }
        let candidates: Vec<usize> = (0..self.stories.len()).filter(|&i| self.stories[i].float.is_none() && want(self.stories[i].story)).collect();
        // The smallest box containing the point: a note area inside the body's box is the note's.
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

    /// The deepest cell of `story` containing the point.
    pub fn cell_at(&self, story: usize, x: f64, y: f64) -> Option<&GCell> {
        self.cells
            .iter()
            .filter(|c| c.story == story && x >= c.rect[0] && x < c.rect[2] && y >= c.rect[1] && y < c.rect[3])
            .max_by_key(|c| c.chain.len())
    }

    /// The table selection zone under a point, sizes in points (`px`: one pixel at the zoom): a cell's zone is 2 to
    /// 8 px right of its left border, a row's from 3 px left of its table across the margin, a column's the top 4 px
    /// inside the first row. The innermost table's zones win.
    pub fn zone(&self, x: f64, y: f64, px: f64) -> Option<Zone> {
        let deepest = |v: &mut Vec<(usize, Zone)>| v.iter().max_by_key(|(d, _)| *d).map(|(_, z)| *z);
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
        // The move handle: up to 12 px above a table's first row and from 12 px left of its left edge to 2 px in.
        for r in self.rows.iter().filter(|r| r.row == 0 && !r.repeat) {
            let [x0, y0, ..] = r.rect;
            if y < y0 && y >= y0 - 12.0 * px && x >= x0 - 12.0 * px && x < x0 + 2.0 * px {
                found.push((r.outer.len(), Zone::Table(r.table)));
            }
        }
        if let Some(z) = deepest(&mut found) {
            return Some(z);
        }
        // Row zones: left of a row's table (its leftmost cell), down to the story's or containing cell's left.
        for r in &self.rows {
            if y < r.rect[1] || y >= r.rect[3] {
                continue;
            }
            let left = self
                .cells
                .iter()
                .filter(|c| c.chain.last().is_some_and(|k| k.0 == r.table && k.1 == r.row))
                .map(|c| c.rect[0])
                .fold(f64::INFINITY, f64::min);
            if !left.is_finite() || x > left - 3.0 * px {
                continue;
            }
            // Outside the cell that holds a nested table, the outer row's zone applies.
            let outer_left = r.outer.last().and_then(|k| self.cells.iter().find(|c| c.chain.last() == Some(k))).map_or(0.0, |c| c.rect[0]);
            if x >= outer_left {
                found.push((r.outer.len(), Zone::Row(r.table, r.row)));
            }
        }
        deepest(&mut found)
    }

    /// A point in the selection bar (from the page's left edge to `left`, the text column's left edge
    /// less 2 px): the body line beside it, nearest vertically. Beside a table row it is the row's zone, not the bar.
    pub fn selection_bar(&self, x: f64, y: f64, left: f64) -> Option<usize> {
        // The body (of the section) beside the point.
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

    /// The text position under a point: in the deepest cell containing it, the nearest line of exactly that
    /// cell's chain (or the cell's closing paragraph), then the nearest stop. Past a line's end the caret sits at the
    /// end with upstream affinity.
    pub fn hit_text(&self, x: f64, y: f64, want: &dyn Fn(StoryId) -> bool) -> Option<TextHit> {
        let si = self.story_at(x, y, want)?;
        let story = self.stories[si].story;
        let mut cell = self.cell_at(si, x, y);
        // A vertical merge's continuation row: its merged cell's text, in the row that starts the merge.
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
        // Below an inner table in its cell, or a cell whose only lines are in nested tables: the closing paragraph.
        if let Some(c) = cell
            && let Some((p, [_, top, _])) = c.closing
            && best.is_none_or(|l| y > l.bottom && top <= y + 1.0)
        {
            return Some(TextHit { story, para: p, unit: Unit(0), affinity: Affinity::Downstream, repeat: false });
        }
        let line = match best {
            Some(l) => l,
            // A cell with no lines of its own: the nearest line inside it.
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

/// Vertical distance first, then horizontal (a click beside a line picks that line).
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

/// The geometry of a snapshot's pages, built on first use and kept by page fragment identity across snapshots
/// (a reused page keeps its geometry).
#[derive(Default)]
pub struct Pages {
    snapshot: Option<Rc<crate::layout::Snapshot>>,
    by_page: Vec<Option<Rc<PageGeometry>>>,
    cache: std::collections::HashMap<crate::layout::FragmentId, Rc<PageGeometry>>,
}

impl Pages {
    /// Switches to a newer snapshot: geometry of pages it shares by identity is kept, the rest is dropped.
    pub fn sync(&mut self, snapshot: &Rc<crate::layout::Snapshot>) {
        if self.snapshot.as_ref().is_some_and(|s| Rc::ptr_eq(s, snapshot)) {
            return;
        }
        let ids: HashSet<crate::layout::FragmentId> = snapshot.pages().iter().map(|p| p.id).collect();
        self.cache.retain(|id, _| ids.contains(id));
        self.by_page = vec![None; snapshot.pages().len()];
        self.snapshot = Some(snapshot.clone());
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

    /// Page `i`'s geometry; `None` past the end or for a provisional page.
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
                let g = Rc::new(PageGeometry::build(frag, idx)?);
                self.cache.insert(frag.id, g.clone());
                g
            }
        };
        self.by_page[i] = Some(g.clone());
        Some(g)
    }

    /// The page and line showing unit `u` of `para` with `affinity`, searching from `near`. Where a paragraph continues
    /// on the next page, the unit that ends one page's last line starts the next page's first: downstream, the caret
    /// shows there.
    pub fn line_showing(
        &mut self,
        para: ParaId,
        u: Unit,
        affinity: Affinity,
        near: usize,
        idx: &DocIndex,
    ) -> Option<(usize, Rc<PageGeometry>, usize)> {
        let pages = self.pages_of(para, near, idx);
        for (k, &page) in pages.iter().enumerate() {
            let Some(geo) = self.page(page, idx) else { continue };
            let Some(i) = geo.line_at(para, u, affinity) else { continue };
            let l = &geo.lines[i];
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
        None
    }

    /// The pages showing `para`, searching outward from `near` (where the caret was last).
    pub fn pages_of(&mut self, para: ParaId, near: usize, idx: &DocIndex) -> Vec<usize> {
        let n = self.len();
        // `near` may be from before the document got shorter.
        let near = near.min(n.saturating_sub(1));
        let mut out = Vec::new();
        for d in 0..n {
            for i in [near.checked_sub(d), (d > 0).then_some(near + d)].into_iter().flatten() {
                if i < n && self.page(i, idx).is_some_and(|g| g.shows_para(para)) {
                    out.push(i);
                }
            }
            // A paragraph's pages are contiguous: once found, stop at the first page past them on both sides.
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
