//! Tables: column widths (TABLE-4 to TABLE-22), cell content, row heights (TABLE-27 to TABLE-32), borders
//! (TABLE-36), and drawing a row as a [`RowFragment`]. Rows are paginated by the column fill (TABLE-40 to TABLE-47).

use crate::docx::Compat;
use std::rc::Rc;

use crate::docx::model::*;
use crate::docx::props::{Border, Jc, LineRule, Width};
use crate::docx::review::{MarkupView, ReviewView};

use super::autofit;
use super::cache::{CacheKey, cached};
use super::env::TextEnv;
use super::fragment::{BoxKind, CellFragment, CellPlace, Child, GlyphRun, Item, LineFragment, PlacedLine, RowFragment, TableFragment};
use super::ids::{ParaId, TableId, Unit};
use super::job::{Memos, Yield};
use super::join::LaidBlock;
use super::line::{break_lines, neighbour_borders};
use super::space::ParaSpace;
use super::story::{CellEnds, NestedRow, StoryEnv, StoryKey, StoryLayout, StoryUnit, child_ids};
use super::text::{Kind, shape};
use super::units::Lu;

const EPS: f64 = 1e-6;

/// What a table's layout depends on: the table's own properties (its cells' content is in `cells`), the
/// available width, the column widths, each cell's story by identity (`None`: not laid out), and the `TextEnv` and
/// font revision.
#[derive(Debug)]
pub struct TableKey {
    shell: Table,
    text_w: u64,
    measure: bool,
    cols: Vec<f64>,
    /// The table's autofit minimum, width and hard minimum: a measured table's key has no cells, and what a cell
    /// holding it needs comes from these, not from the columns.
    fit: (i64, i64, i64),
    cells: Vec<Option<Rc<StoryLayout>>>,
    text: TextEnv,
    fonts: u64,
}

/// A [`TableKey`] borrowed.
pub struct TableProbe<'a> {
    table: &'a Table,
    text_w: u64,
    measure: bool,
    cols: &'a [f64],
    fit: (i64, i64, i64),
    cells: &'a [Option<Rc<StoryLayout>>],
    text: &'a TextEnv,
    fonts: u64,
}

fn same_story(a: &Option<Rc<StoryLayout>>, b: &Option<Rc<StoryLayout>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => Rc::ptr_eq(x, y),
        _ => false,
    }
}

impl CacheKey for TableKey {
    type Probe<'a> = TableProbe<'a>;

    fn bucket(p: &TableProbe) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = super::cache::KeyHasher::default();
        (p.table.id, p.text_w, p.measure, p.cells.len()).hash(&mut h);
        for c in p.cells {
            c.as_ref().map(Rc::as_ptr).hash(&mut h);
        }
        h.finish()
    }

    fn matches(&self, p: &TableProbe) -> bool {
        self.text_w == p.text_w
            && self.measure == p.measure
            && self.fonts == p.fonts
            && self.cols == p.cols
            && self.fit == p.fit
            && self.text == *p.text
            && self.cells.len() == p.cells.len()
            && self.cells.iter().zip(p.cells).all(|(a, b)| same_story(a, b))
            && same_shell(&self.shell, p.table)
    }

    fn from_probe(p: &TableProbe) -> TableKey {
        TableKey {
            shell: shell(p.table),
            text_w: p.text_w,
            measure: p.measure,
            cols: p.cols.to_vec(),
            fit: p.fit,
            cells: p.cells.to_vec(),
            text: p.text.clone(),
            fonts: p.fonts,
        }
    }
}

/// A table's properties without its cells' content (which its key holds by identity). The destructuring is
/// exhaustive, so a field added to the model must be decided on here and in [`same_shell`].
fn shell(t: &Table) -> Table {
    let Table {
        style_id,
        width,
        jc,
        ind,
        ind_set,
        borders,
        fixed,
        cell_spacing,
        look,
        bands,
        floating,
        bidi_visual,
        grid,
        rows,
        src: _,
        id,
        direct: _,
        controls: _,
        change,
        grid_change,
    } = t;
    let rows = rows
        .iter()
        .map(|r| {
            let Row { height, cant_split, header, grid_before, grid_after, w_before, w_after, ind, jc, cells, src: _, controls: _, rev } = r;
            let cells = cells
                .iter()
                .map(|c| {
                    let Cell {
                        width,
                        span,
                        v_merge,
                        top,
                        left,
                        bottom,
                        right,
                        shading,
                        margins,
                        v_align,
                        text_direction,
                        no_wrap,
                        fit_text,
                        blocks: _,
                        direct: _,
                        controls: _,
                        rev,
                    } = c;
                    Cell {
                        width: *width,
                        span: *span,
                        v_merge: *v_merge,
                        top: top.clone(),
                        left: left.clone(),
                        bottom: bottom.clone(),
                        right: right.clone(),
                        shading: *shading,
                        margins: *margins,
                        v_align: v_align.clone(),
                        text_direction: text_direction.clone(),
                        no_wrap: *no_wrap,
                        fit_text: *fit_text,
                        blocks: Vec::new(),
                        direct: Default::default(),
                        controls: Vec::new(),
                        rev: rev.clone(),
                    }
                })
                .collect();
            Row {
                height: height.clone(),
                cant_split: *cant_split,
                header: *header,
                grid_before: *grid_before,
                grid_after: *grid_after,
                w_before: *w_before,
                w_after: *w_after,
                ind: *ind,
                jc: *jc,
                cells,
                src: None,
                controls: Vec::new(),
                rev: rev.clone(),
            }
        })
        .collect();
    Table {
        style_id: style_id.clone(),
        width: *width,
        jc: *jc,
        ind: *ind,
        ind_set: *ind_set,
        borders: borders.clone(),
        fixed: *fixed,
        cell_spacing: *cell_spacing,
        look: *look,
        bands: *bands,
        floating: *floating,
        bidi_visual: *bidi_visual,
        grid: grid.clone(),
        rows,
        src: None,
        id: *id,
        direct: Default::default(),
        controls: Vec::new(),
        change: change.clone(),
        grid_change: grid_change.clone(),
    }
}

/// Whether `t`'s properties equal the shell's (cells' content not compared).
pub(crate) fn same_shell(s: &Table, t: &Table) -> bool {
    let Table {
        style_id,
        width,
        jc,
        ind,
        ind_set,
        borders,
        fixed,
        cell_spacing,
        look,
        bands,
        floating,
        bidi_visual,
        grid,
        rows,
        src: _,
        id,
        direct: _,
        controls: _,
        change,
        grid_change,
    } = s;
    *style_id == t.style_id
        && *width == t.width
        && *jc == t.jc
        && *ind == t.ind
        && *ind_set == t.ind_set
        && *borders == t.borders
        && *fixed == t.fixed
        && *cell_spacing == t.cell_spacing
        && *look == t.look
        && *bands == t.bands
        && *floating == t.floating
        && *bidi_visual == t.bidi_visual
        && *grid == t.grid
        && *id == t.id
        && *change == t.change
        && *grid_change == t.grid_change
        && rows.len() == t.rows.len()
        && rows.iter().zip(&t.rows).all(|(r, q)| {
            let Row { height, cant_split, header, grid_before, grid_after, w_before, w_after, ind, jc, cells, src: _, controls: _, rev } = r;
            *height == q.height
                && *cant_split == q.cant_split
                && *header == q.header
                && *grid_before == q.grid_before
                && *grid_after == q.grid_after
                && *w_before == q.w_before
                && *w_after == q.w_after
                && *ind == q.ind
                && *jc == q.jc
                && *rev == q.rev
                && cells.len() == q.cells.len()
                && cells.iter().zip(&q.cells).all(|(c, d)| {
                    let Cell {
                        width,
                        span,
                        v_merge,
                        top,
                        left,
                        bottom,
                        right,
                        shading,
                        margins,
                        v_align,
                        text_direction,
                        no_wrap,
                        fit_text,
                        blocks: _,
                        direct: _,
                        controls: _,
                        rev,
                    } = c;
                    *width == d.width
                        && *span == d.span
                        && *v_merge == d.v_merge
                        && *top == d.top
                        && *left == d.left
                        && *bottom == d.bottom
                        && *right == d.right
                        && *shading == d.shading
                        && *margins == d.margins
                        && *v_align == d.v_align
                        && *text_direction == d.text_direction
                        && *no_wrap == d.no_wrap
                        && *fit_text == d.fit_text
                        && *rev == d.rev
                })
        })
}

// shortcut: entry sizes are rough (the structs, not what they share); refine with memory budgets.
fn table_bytes(t: &TableLayout) -> usize {
    std::mem::size_of::<TableLayout>()
        + t.rows.iter().map(|r| std::mem::size_of::<RowBox>() + r.cells.len() * std::mem::size_of::<CellBox>()).sum::<usize>()
        + t.fit.iter().map(|r| r.cells.len() * std::mem::size_of::<autofit::Cell>()).sum::<usize>()
}

/// A cell's laid-out content.
#[derive(Clone, Debug, PartialEq)]
pub struct CellBox {
    /// Left border centre, relative to the table's left border centre.
    pub x: f64,
    /// Border centre to border centre.
    pub w: f64,
    /// Content box: left offset from `x` and width (TABLE-24).
    pub cx: f64,
    /// The width its content was laid out at (an edit in another row reuses `units` at this width).
    pub cw: f64,
    pub margins: [f64; 4],
    pub units: Rc<StoryLayout>,
    pub content_h: f64,
    /// The content height TABLE-30 aligns: `content_h`, or lower, the bottom of a float that counts in it.
    pub align_h: f64,
    /// Floats anchored in the cell but not laid out in it (compat ≤ 14, layoutInCell="0"): the alignment offset
    /// doesn't move them (FLOAT-14).
    pub fixed_floats: Box<[u64]>,
    pub v_align: String,
    pub v_merge: VMerge,
    /// The grid columns this cell spans.
    pub col: usize,
    pub span: usize,
    /// Resolved edges (TABLE-36): top, left, bottom, right; shared with the cell's geometry.
    pub edges: Rc<[Option<Border>; 4]>,
    pub shading: Option<u32>,
    /// A merge's first cell: the height of the rows it covers beyond its own row (TABLE-31).
    pub below: f64,
    /// The cell's index in its model row.
    pub index: usize,
    /// A closing empty paragraph after a nested table, which takes no space (TABLE-26): its id and its mark's
    /// font size, for the caret place editing shows there.
    pub closing: Option<(ParaId, f64)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RowBox {
    pub cells: Vec<CellBox>,
    /// TABLE-27: the widest top border among the row's cells.
    pub top_border: f64,
    /// The row's pitch (top border included).
    pub height: f64,
    pub cant_split: bool,
    /// TABLE-45: the row's first paragraph has keepNext: the row doesn't split and keeps with the next row.
    pub keep_next: bool,
    /// TABLE-45: the row's first paragraph has pageBreakBefore: a row after the table's first starts the next column.
    pub break_before: bool,
    pub header: bool,
    pub exact: bool,
    /// An atLeast row's minimum height (top border included), else 0: the row splits only where it fits.
    pub min_h: f64,
    /// Cell spacing (TABLE-39): from the row top to its cell boxes (2s, plus the table's top border on row 0).
    pub gap_top: f64,
    /// Bottom borders of the row above whose cells no cell of this row starts under (their edge was resolved against
    /// the cell below, which draws its own top from another cell, TABLE-38): left, right (table coordinates) and
    /// border.
    pub seams: Vec<(f64, f64, Border)>,
    /// The markup view hides the row (REV-1): it has no cells and takes no space, and the fill skips it.
    pub hidden: bool,
    /// The row or one of its cells was inserted, deleted or changed, and the view draws change bars (REV-4).
    pub changed: bool,
}

impl TableLayout {
    /// Whether row `ri` is the last one shown (the rows after it are hidden by the markup view, REV-1).
    pub fn is_last(&self, ri: usize) -> bool {
        self.rows.get(ri + 1..).is_none_or(|rest| rest.iter().all(|r| r.hidden))
    }
}

/// A table laid out for a width.
#[derive(Debug, PartialEq)]
pub struct TableLayout {
    /// Left border centre, relative to the column's left edge.
    pub x0: f64,
    pub cols: Vec<f64>,
    pub rows: Vec<RowBox>,
    /// TABLE-27: the last row's bottom border.
    pub bottom_border: f64,
    /// `w:tblCellSpacing` and the table's outer borders (top, left, bottom, right), drawn separately with spacing.
    pub spacing: f64,
    pub id: TableId,
    pub outer: [Option<Border>; 4],
    /// Each row's cells' autofit inputs (width-free): an edit in other rows reuses them.
    pub fit: Rc<[FitRow]>,
    /// Drawn right to left (TABLE-23): the cells' `x` and `cx` are as drawn, their `col`, `edges` and the spacing
    /// gaps logical.
    pub rtl: bool,
}

/// A row's cells' autofit inputs, and the cached content widths they came from: held while the layout is, so eviction
/// keeps them for the table's next lookup.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FitRow {
    cells: Vec<autofit::Cell>,
    widths: Vec<Rc<(i64, i64, i64)>>,
}

fn style_factor(style: &str) -> f64 {
    match style {
        "double" => 3.0,
        "triple" => 3.5,
        "dashed" | "dotted" | "dashSmallGap" | "dotDash" | "dotDotDash" => 0.75,
        _ => 1.0,
    }
}

/// ST_Border's line styles in enumeration order (ties: the later style wins, TABLE-36).
const ORDER: [&str; 27] = [
    "nil",
    "none",
    "single",
    "thick",
    "double",
    "dotted",
    "dashed",
    "dotDash",
    "dotDotDash",
    "triple",
    "thinThickSmallGap",
    "thickThinSmallGap",
    "thinThickThinSmallGap",
    "thinThickMediumGap",
    "thickThinMediumGap",
    "thinThickThinMediumGap",
    "thinThickLargeGap",
    "thickThinLargeGap",
    "thinThickThinLargeGap",
    "wave",
    "doubleWave",
    "dashSmallGap",
    "dashDotStroked",
    "threeDEmboss",
    "threeDEngrave",
    "outset",
    "inset",
];

fn style_rank(style: &str) -> usize {
    ORDER.iter().position(|s| *s == style).unwrap_or(2)
}

/// TABLE-37: an art border (any style that isn't a line style) is `w:sz` points wide in layout.
fn art(b: &Border) -> Option<f64> {
    (!ORDER.contains(&b.style.as_str())).then(|| (b.width * 8.0).min(255.0))
}

/// TABLE-36: which of two borders meeting on an edge is drawn. `(border, cell_level)`.
fn conflict(a: (Option<&Border>, bool), b: (Option<&Border>, bool)) -> Option<Border> {
    winner(a, b).cloned()
}

/// [`conflict`]'s winner, borrowed: only the border drawn is copied.
fn winner<'a>(a: (Option<&'a Border>, bool), b: (Option<&'a Border>, bool)) -> Option<&'a Border> {
    let real = |x: Option<&'a Border>| x.filter(|b| b.visible());
    let (ra, rb) = (real(a.0), real(b.0));
    match (ra, rb) {
        (None, None) => None,
        (Some(x), None) => Some(x),
        (None, Some(y)) => Some(y),
        (Some(x), Some(y)) => {
            if a.1 != b.1 {
                return Some(if a.1 { x } else { y });
            }
            let (wx, wy) = (x.width * style_factor(&x.style), y.width * style_factor(&y.style));
            if (wx - wy).abs() > EPS {
                return Some(if wx > wy { x } else { y });
            }
            let (sx, sy) = (style_rank(&x.style), style_rank(&y.style));
            if sx != sy {
                return Some(if sx > sy { x } else { y });
            }
            let key = |c: Option<u32>| {
                let c = c.unwrap_or(0);
                let (r, g, b) = ((c >> 16) & 255, (c >> 8) & 255, c & 255);
                (r + b + 2 * g, b + 2 * g, g)
            };
            Some(if key(x.color) <= key(y.color) { x } else { y })
        }
    }
}

fn bw(b: &Option<Border>) -> f64 {
    b.as_ref().filter(|b| b.visible()).map_or(0.0, thick)
}

/// A border's thickness (TABLE-37).
fn thick(b: &Border) -> f64 {
    stripes(b).iter().sum()
}

/// TABLE-37: the space a border takes across a row boundary in layout, and (TABLE-17, TABLE-23, TABLE-24) its width
/// in horizontal geometry (cell boxes, edge allowances, x0): `w:space` plus its line (twice with
/// `w:shadow`), the line's width rounded down to whole twips; wave, doubleWave and dashDotStroked take a fixed height,
/// threeDEmboss and threeDEngrave 1.5pt more (3pt from 3pt wide).
/// Art borders count their art width.
fn space_h(b: &Option<Border>) -> f64 {
    let Some(b) = b.as_ref().filter(|b| b.visible()) else { return 0.0 };
    if let Some(a) = art(b) {
        return b.space.clamp(0.0, 31.0) + if b.shadow { 2.0 } else { 1.0 } * a;
    }
    let w = (b.width.clamp(0.25, 12.0) * 20.0 + 1e-6).floor() / 20.0;
    let line = match b.style.as_str() {
        "wave" if (b.width - 1.5).abs() < EPS => 3.75,
        "wave" | "dashDotStroked" => 3.0,
        "doubleWave" => 5.25,
        "threeDEmboss" | "threeDEngrave" => w + if b.width < 3.0 { 1.5 } else { 3.0 },
        _ if stripes(b).len() == 1 => w,
        _ => thick(b),
    };
    b.space.clamp(0.0, 31.0) + if b.shadow { 2.0 } else { 1.0 } * line
}

/// TABLE-24: a border's width for the cell content box: its layout width, an art border's as a single line.
fn box_h(b: &Option<Border>) -> f64 {
    match b.as_ref().filter(|b| b.visible() && art(b).is_some()) {
        Some(b) => b.space.clamp(0.0, 31.0) + if b.shadow { 2.0 } else { 1.0 } * ((b.width * 20.0 + 1e-6).floor() / 20.0).max(0.05),
        None => space_h(b),
    }
}

/// A compound border's lines and gaps across its thickness, top or left first (lines at even indices), as Word lays
/// them out and draws them: a thinThick border's thick line is on top and on the left on every side.
/// shortcut: the fixed 0.75 pt and 1.5 pt widths were measured at w:sz 12 and 24 only.
fn stripes(b: &Border) -> Vec<f64> {
    let (w, s) = (b.width, b.style.as_str());
    let (thin, gap, thick) = if s.ends_with("SmallGap") {
        (0.75, 0.75, w)
    } else if s.ends_with("MediumGap") {
        (w / 2.0, w / 2.0, w)
    } else if s.ends_with("LargeGap") {
        (0.75, w, 1.5)
    } else {
        (w, w, w)
    };
    if let Some(a) = art(b) {
        return vec![w, (a - w).max(0.0)];
    }
    match s {
        "threeDEmboss" | "threeDEngrave" => {
            let o = if w < 3.0 { 0.75 } else { 1.5 };
            vec![o, w, o]
        }
        "double" => vec![w, w, w],
        "triple" => vec![w; 5],
        _ if s.starts_with("thinThickThin") => vec![thin, gap, thick, gap, thin],
        _ if s.starts_with("thinThick") => vec![thick, gap, thin],
        _ if s.starts_with("thickThin") => vec![thin, gap, thick],
        _ => vec![w],
    }
}

/// Draws border `b` filling `rect` (its thickness across, horizontal or vertical): a compound border as its lines.
fn edge(out: &mut Vec<Item>, rect: [f64; 4], horizontal: bool, b: &Border) {
    let colour = b.color.unwrap_or(0);
    let [x0, y0, x1, y1] = rect;
    let st = stripes(b);
    // TABLE-37: a 3D line is three lines across its band, top or left first: light, middle, dark when embossed, dark,
    // middle, light when engraved.
    if let Some(cs) = three_d(b) {
        let mut at = if horizontal { y0 } else { x0 };
        for (w, colour) in st.iter().zip(cs) {
            out.push(Item::rect(if horizontal { [x0, at, x1, at + w] } else { [at, y0, at + w, y1] }, colour));
            at += w;
        }
        return;
    }
    if st.len() == 1 && !dash_pattern(&b.style).is_empty() {
        out.extend(dashes(&b.style, rect, horizontal).into_iter().map(|rect| Item::rect(rect, colour)));
        return;
    }
    if st.len() == 1 {
        out.push(Item::rect(rect, colour));
        return;
    }
    let mut at = if horizontal { y0 } else { x0 };
    for (k, w) in st.iter().enumerate() {
        if k % 2 == 0 {
            out.push(Item::rect(if horizontal { [x0, at, x1, at + w] } else { [at, y0, at + w, y1] }, colour));
        }
        at += w;
    }
}

/// TABLE-37, PARA-7: a patterned line's dashes, on and off in units of its width (a dashSmallGap's of the band's
/// thickness, the same for a single line); empty for a solid line.
/// shortcut: wave, doubleWave and dashDotStroked are drawn as solid lines.
pub(super) fn dash_pattern(style: &str) -> &'static [f64] {
    match style {
        "dotted" => &[1.0, 1.0],
        "dashed" => &[3.0, 1.0],
        "dotDash" => &[3.0, 1.0, 1.0, 1.0],
        "dotDotDash" => &[3.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        "dashSmallGap" => &[4.0, 1.0],
        _ => &[],
    }
}

/// The dashes of a patterned line filling `rect` (its thickness across), from the band's start.
pub(super) fn dashes(style: &str, rect: [f64; 4], horizontal: bool) -> Vec<[f64; 4]> {
    let [x0, y0, x1, y1] = rect;
    let unit = if horizontal { y1 - y0 } else { x1 - x0 };
    let (start, end) = if horizontal { (x0, x1) } else { (y0, y1) };
    if unit <= EPS {
        return vec![rect];
    }
    let mut out = Vec::new();
    let mut at = start;
    for (k, len) in dash_pattern(style).iter().cycle().enumerate() {
        if at >= end - EPS {
            break;
        }
        let to = (at + len * unit).min(end);
        if k % 2 == 0 {
            out.push(if horizontal { [at, y0, to, y1] } else { [x0, at, x1, to] });
        }
        at = to;
    }
    out
}

/// TABLE-37: a threeDEmboss or threeDEngrave border's three colours, top or left first, from its colour (auto as
/// A0A0A0) on the 0–240 HLS scale: black gives C0C0C0, 808080, 000000 and white FFFFFF, C0C0C0, 808080; other colours
/// take L' = L + 40 below 60, L below 120, else L − 40, and lines at L' + 40, L', L' − 40. The middle line is drawn
/// in the average of its colour and that colour 60 darker (white's C0C0C0 and black as they are).
fn three_d(b: &Border) -> Option<[u32; 3]> {
    let engrave = match b.style.as_str() {
        "threeDEmboss" => false,
        "threeDEngrave" => true,
        _ => return None,
    };
    let c = b.color.unwrap_or(0xA0A0A0);
    let (light, middle, dark) = match c {
        0x000000 => (0xC0C0C0, 0x808080, 0x000000),
        0xFFFFFF => (0xFFFFFF, 0xC0C0C0, 0x808080),
        _ => {
            let (h, l, s) = to_hls(c);
            let l2 = if l < 60 {
                l + 40
            } else if l < 120 {
                l
            } else {
                l - 40
            }
            .clamp(0, 255);
            let mid = from_hls(h, l2, s);
            let shade = from_hls(h, (l2 - 60).max(0), s);
            let avg = (0..3).fold(0, |acc, k| {
                let (a, b) = ((mid >> (8 * k)) & 0xFF, (shade >> (8 * k)) & 0xFF);
                acc | (((a * 8 + b * 8 + 8) >> 4) << (8 * k))
            });
            (from_hls(h, (l2 + 40).min(240), s), avg, from_hls(h, (l2 - 40).max(0), s))
        }
    };
    Some(if engrave { [dark, middle, light] } else { [light, middle, dark] })
}

/// RGB (0xRRGGBB) to hue, lightness and saturation on the 0–240 scale, in integers (TABLE-37).
fn to_hls(c: u32) -> (i64, i64, i64) {
    let (r, g, b) = (i64::from((c >> 16) & 0xFF), i64::from((c >> 8) & 0xFF), i64::from(c & 0xFF));
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = ((max + min) * 240 + 255) / 510;
    if max == min {
        return (160, l, 0);
    }
    let d = max - min;
    let s = if l < 121 { ((max + min) / 2 + d * 240) / (max + min) } else { ((510 - max - min) / 2 + d * 240) / (510 - max - min) };
    let part = |v: i64| (d / 2 + (max - v) * 40) / d;
    let h = if r == max {
        part(b) - part(g)
    } else if g == max {
        part(r) - part(b) + 80
    } else {
        part(g) - part(r) + 160
    };
    (h.rem_euclid(240), l, s)
}

/// Hue, lightness and saturation (0–240) to RGB (TABLE-37).
fn from_hls(h: i64, l: i64, s: i64) -> u32 {
    let scale = |v: i64| ((v * 255 + 120) / 240).clamp(0, 255) as u32;
    if s == 0 {
        let v = scale(l);
        return (v << 16) | (v << 8) | v;
    }
    let m2 = if l < 121 { ((s + 240) * l + 120) / 240 } else { l - (l * s + 120) / 240 + s };
    let m1 = 2 * l - m2;
    let channel = |h: i64| {
        let h = h.rem_euclid(240);
        if h < 40 {
            m1 + ((m2 - m1) * h + 20) / 40
        } else if h < 120 {
            m2
        } else if h < 160 {
            m1 + ((m2 - m1) * (160 - h) + 20) / 40
        } else {
            m1
        }
    };
    (scale(channel(h + 80)) << 16) | (scale(channel(h)) << 8) | scale(channel(h - 80))
}

/// TABLE-17: half a border in whole twips, rounded up.
fn half_border(w: f64) -> f64 {
    let tw = (w * 20.0).round() as i64;
    ((tw + 1) / 2) as f64 / 20.0
}

/// TABLE-49: a table's outer box, left and right relative to the column, widest over its rows: from the first cell's
/// left border outer edge (half rounded up) to the last cell's right border outer edge (half).
pub fn outer_box(tb: &TableLayout) -> (f64, f64) {
    // TABLE-23: a right-to-left row's first cell is on its right.
    let ends = |r: &RowBox| {
        let last = r.cells.len().checked_sub(1)?;
        Some(if tb.rtl { (last, 0) } else { (0, last) })
    };
    tb.rows.iter().filter_map(|r| ends(r).map(|(a, z)| (&r.cells[a], &r.cells[z]))).fold((f64::INFINITY, f64::NEG_INFINITY), |(l, r), (a, z)| {
        let (al, zr) = (drawn_edges(tb, a)[1], drawn_edges(tb, z)[3]);
        (l.min(tb.x0 + a.x - half_border(bw(al))), r.max(tb.x0 + z.x + z.w + bw(zr) / 2.0))
    })
}

/// TABLE-25: what a cell story shifts a nested table right by, the difference [`assemble`]'s compat <= 14 position
/// leaves. Left-aligned and right-to-left tables only.
pub fn nested_shift(t: &Table) -> f64 {
    if rtl(t) {
        // TABLE-23: a nested right-to-left table takes the compat 15 placement.
        let edges = cell_edges(t, grid_columns(t), &|_| None);
        let res = rtl_sides(edges.first().into_iter().flatten().map(|c| &*c.edges));
        return rtl_x0(t, 0.0, 0.0, res, false) - rtl_x0(t, 0.0, 0.0, res, true);
    }
    // (A nested table's negative indent already counts as 0: `story::as_nested`.)
    if matches!(t.jc, Jc::Center | Jc::Right) {
        return 0.0;
    }
    // The first cell's own left border, nil included, else the table's (TABLE-25).
    let (lw, lmar) = t
        .rows
        .first()
        .and_then(|r| r.cells.first())
        .map_or((space_h(&t.borders[1]), 0.0), |c| (space_h(candidate(Some(&c.left), &t.borders[1])), c.margins[1]));
    lw / 2.0 + edge_gap(t, lw, lmar)
}

/// TABLE-23: whether the table is drawn right to left (`w:bidiVisual`; a floating table isn't).
fn rtl(t: &Table) -> bool {
    t.bidi_visual && t.floating.is_none()
}

/// TABLE-23: the resolved right border of the first row's first cell and the resolved left border of its last, in
/// twips (a right-to-left table's inner sides of its outer cells, with cell spacing).
fn rtl_sides<'a>(mut row: impl Iterator<Item = &'a [Option<Border>; 4]>) -> (i64, i64) {
    let first = row.next();
    let last = row.last().or(first);
    (first.map_or(0, |e| twips(space_h(&e[3]))), last.map_or(0, |e| twips(space_h(&e[1]))))
}

/// TABLE-23: a right-to-left table's left border centre (its logical last edge) from the column's left edge, for a
/// table `total` wide in a column `text_w` wide. `res`: [`rtl_sides`]. `sloppy`: the compat <= 14 placement.
fn rtl_x0(t: &Table, total: f64, text_w: f64, res: (i64, i64), sloppy: bool) -> f64 {
    let row = t.rows.first();
    let (first, last) = (row.and_then(|r| r.cells.first()), row.and_then(|r| r.cells.last()));
    let side = |own: Option<&Border>, table: &Option<Border>| twips(space_h(&conflict((own, true), (table.as_ref(), false))));
    // The table's left and right borders are its start and end: its right and left sides as drawn.
    let b_start = side(first.and_then(|c| c.left.as_ref()), &t.borders[1]);
    let b_end = side(last.and_then(|c| c.right.as_ref()), &t.borders[3]);
    // The last cell is the first too: its left is the table's.
    let alone = row.is_some_and(|r| r.grid_before == 0 && r.cells.len() == 1);
    let first_outer = row.is_some_and(|r| r.grid_after == 0 && r.cells.len() == 1);
    let b_last = side(last.and_then(|c| c.left.as_ref()), &t.borders[if alone { 1 } else { 5 }]);
    let hb = |b: i64| (b + 1) / 2;
    let (w, width, ind, s) = (twips(text_w), twips(total), twips(t.ind), twips(t.cell_spacing.max(0.0)));
    let margin = |c: Option<&Cell>| c.map_or(0, |c| twips(c.margins[1]));
    // TABLE-39: with cell spacing a cell's gap on its left as drawn holds the spacing and border of its logical right.
    let gap = |outer: bool, b: i64| if s > 0 { s * if outer { 2 } else { 1 } + b } else { 0 };
    let extra = (b_start + b_last) / 2;
    let left = match t.jc {
        // jc right (end): at the left edge, the indent ignored.
        Jc::Right if sloppy => -hb(b_end).max(margin(last) + gap(alone, res.1)),
        Jc::Right => 0,
        Jc::Center if sloppy => (w - width).div_euclid(2),
        Jc::Center => (w - width - extra).div_euclid(2),
        // jc absent, left (start), both: at the right edge, the indent from it.
        _ if sloppy => w - ind + hb(b_start).max(margin(first) + gap(first_outer, res.0)) - width,
        _ => w - ind - extra - width,
    };
    left as f64 / 20.0
}

/// TABLE-36: a cell side's candidate border: the cell's own when it writes that side (`nil` included), else the
/// table's.
fn candidate<'b>(own: Option<&'b Option<Border>>, table: &'b Option<Border>) -> &'b Option<Border> {
    match own {
        Some(o @ Some(_)) => o,
        _ => table,
    }
}

/// TABLE-17: the per-side edge allowance added to the available width (left, right).
fn edge_add(compat: Compat, t: &Table) -> (f64, f64) {
    let first = t.rows.first();
    let (mut lc, mut rc) = (first.and_then(|r| r.cells.first()), first.and_then(|r| r.cells.last()));
    // TABLE-23: a right-to-left table's first and last cells swap, each still against the table's own side.
    if rtl(t) {
        (lc, rc) = (rc, lc);
    }
    let lw = space_h(candidate(lc.map(|c| &c.left), &t.borders[1]));
    let rw = space_h(candidate(rc.map(|c| &c.right), &t.borders[3]));
    if compat == Compat::Generic {
        (-half_border(lw), -half_border(rw))
    } else {
        (edge_gap(t, lw, lc.map_or(0.0, |c| c.margins[1])), edge_gap(t, rw, rc.map_or(0.0, |c| c.margins[3])))
    }
}

/// TABLE-17, TABLE-23 (compat <= 14): the edge allowance of the first cell's left side or the last cell's right side: its gapHalf (the
/// margin; with cell spacing 2s, the border and the margin), at least half the border.
fn edge_gap(t: &Table, b: f64, mar: f64) -> f64 {
    half_border(b).max(if t.cell_spacing > 0.0 { 2.0 * t.cell_spacing + b + mar } else { mar })
}

/// TABLE-39: the cell-spacing gap on a cell side: 2s on the table's outer boundary, s inside.
fn gap(s: f64, outer: bool) -> f64 {
    if outer { 2.0 * s } else { s }
}

/// Content min, max and hard minimum of blocks in twips (TABLE-7), cached and width-free (LayoutNG's
/// min/max sizes).
fn content_widths(blocks: &[Block], env: &StoryEnv, fixed: bool, memos: &mut Memos) -> Result<Rc<(i64, i64, i64)>, Yield> {
    let laid = super::join::laid(blocks, env.ctx.env.text.review.view);
    let cell = Some(CellEnds { fixed, ..CellEnds::default() });
    let probe = StoryKey::new(child_ids(&laid, None, env, cell, memos)?, None, cell, env, &laid);
    cached(memos, |c| &mut c.cell_widths, probe, |_| std::mem::size_of::<(i64, i64, i64)>(), |m| content_widths_uncached(&laid, env, fixed, m))
}

/// TABLE-33: a vertical cell's content thickness in twips: 6, each paragraph's space before and after, and each of its
/// lines' height rounded up to a twip. A vertical line is `em/32` taller than the horizontal one (`em`: the line's
/// largest font size); a line multiple scales that too, an exact line doesn't take it.
fn vertical_width(blocks: &[Block], env: &StoryEnv, memos: &mut Memos) -> Result<i64, Yield> {
    let view = env.ctx.env.text.review.view;
    let mut w = 6;
    for b in &super::join::laid(blocks, view) {
        let LaidBlock::Para(laid) = b else { continue };
        let p = laid.para();
        let space = laid.shape_space_in(env.fields, env.notes, &env.ctx, true);
        let shaped = shape(p, &space, env.ctx.fonts, memos)?;
        let s = p.para_style_in(view);
        let ps = ParaSpace { width: 1e9, start: 0, borders: neighbour_borders(s, None, None), x0: 0.0, env: env.ctx.env.text.clone(), fit: false };
        let pb = break_lines(&shaped, p, &ps, env.ctx.fonts, memos)?;
        w += twips(s.space_before + s.space_after);
        let pieces = pb.pieces();
        let m = (s.line * 240.0).round() as i64;
        for l in &pb.lines {
            let em = pieces[l.start..l.end.min(pieces.len())].iter().map(|q| shaped.styles[q.style as usize].run.size).fold(0.0, f64::max);
            let em = if em > 0.0 { em } else { s.mark_size.unwrap_or(p.mark_in(view).size) };
            let e32 = i64::from(Lu::round_half_up(em / 32.0).0);
            let h = i64::from(Lu::round_half_up(l.pitch).0)
                + match s.line_rule {
                    LineRule::Auto => (e32 * m + 120).div_euclid(240),
                    LineRule::AtLeast => e32,
                    LineRule::Exact => 0,
                };
            // Layout units to twips, rounded up.
            w += (h * 1440 + 294_911) / 294_912;
        }
    }
    Ok(w)
}

/// Points to twips, rounded up (TABLE-6: content widths round up).
fn up(x: f64) -> i64 {
    (x * 20.0 - 1e-6).ceil().max(0.0) as i64
}

/// Points to twips, rounded.
fn twips(x: f64) -> i64 {
    (x * 20.0).round() as i64
}

/// TABLE-7, TABLE-8: the widest unbreakable piece and the widest unwrapped line, with the paragraph's indents.
/// A nested table supplies its own autofit's minimum, width with A = MAXW, and hard minimum (TABLE-11).
fn content_widths_uncached(blocks: &[LaidBlock], env: &StoryEnv, fixed: bool, memos: &mut Memos) -> Result<(i64, i64, i64), Yield> {
    let (mut mn, mut mx, mut hard) = (0i64, 0i64, 0i64);
    let old = env.ctx.env.text.compat == Compat::Legacy;
    for (bi, b) in blocks.iter().enumerate() {
        match b {
            LaidBlock::Para(laid) => {
                let p = laid.para();
                let space = laid.shape_space_in(env.fields, env.notes, &env.ctx, true);
                let shaped = shape(p, &space, env.ctx.fonts, memos)?;
                let s = p.para_style_in(env.ctx.env.text.review.view);
                let ps =
                    ParaSpace { width: 1e9, start: 0, borders: neighbour_borders(s, None, None), x0: 0.0, env: env.ctx.env.text.clone(), fit: false };
                let pb = break_lines(&shaped, p, &ps, env.ctx.fonts, memos)?;
                let ind = if old { s.ind_left + s.ind_right + s.ind_first.min(0.0) } else { s.ind_left + s.ind_right };
                let pieces = pb.pieces();
                // TABLE-9: hyphen runs in the minimum.
                let text_of = |q: &super::text::Piece| shaped.text.get(q.text.0 as usize..q.text.1 as usize).unwrap_or("");
                let mut tail = vec![false; pieces.len() + 1]; // pieces from here on are hyphens or spaces only
                tail[pieces.len()] = true;
                for i in (0..pieces.len()).rev() {
                    let t = text_of(&pieces[i]);
                    tail[i] = tail[i + 1] && (pieces[i].kind == Kind::Space || (!t.is_empty() && t.chars().all(|c| c == '-')));
                }
                // TABLE-8: the list number and its suffix.
                let is_num = |q: &super::text::Piece| q.src.is_none() && matches!(q.kind, Kind::ListNumber | Kind::Tab | Kind::Space);
                let numbered = old && pieces.iter().any(|q| q.kind == Kind::ListNumber);
                // TABLE-8: compat <= 14, a numbered paragraph's first line is laid out from max(left + firstLine, 0).
                let start0 = if numbered { (s.ind_left + s.ind_first).max(0.0) } else { s.ind_left + s.ind_first };
                let relaid;
                let lines: &[super::line::Line] = if numbered && s.ind_left + s.ind_first < 0.0 {
                    let mut st = ParaStyle::clone(s);
                    st.ind_first = -s.ind_left;
                    let mark = p.mark_in(env.ctx.env.text.review.view);
                    relaid = super::line::break_paragraph(&shaped, pieces, &st, mark, &super::line::Frame::new(&ps), env.ctx.fonts, false);
                    &relaid
                } else {
                    &pb.lines
                };
                // TABLE-7: a tab is as wide as it is in the unwrapped lines; `ends`: where each piece ends there.
                let mut laid: Vec<f64> = pieces.iter().map(|q| q.width).collect();
                let mut ends = vec![0.0f64; pieces.len()];
                for l in lines {
                    for (k, (&x, &w)) in l.xs.iter().zip(&l.widths).enumerate() {
                        if let Some(v) = laid.get_mut(l.start + k) {
                            *v = w;
                            ends[l.start + k] = x + w;
                        }
                    }
                }
                let mut word = 0.0f64;
                // TABLE-8: compat <= 14, the number, its suffix and the first word are one piece, from the left edge.
                let mut first = numbered.then_some(start0);
                for (i, pc) in pieces.iter().enumerate() {
                    let sep = matches!(pc.kind, Kind::LineBreak | Kind::PageBreak | Kind::ColumnBreak | Kind::Space | Kind::Tab);
                    if let Some(end) = first {
                        if sep && !is_num(pc) {
                            mn = mn.max(up(end + ind));
                            first = None;
                        } else if pc.kind != Kind::Space {
                            first = Some(ends[i]);
                        }
                        continue;
                    }
                    if is_num(pc) {
                        // TABLE-8: compat 15, the first word's piece starts at the left indent.
                        word = ends[i] - s.ind_left;
                        continue;
                    }
                    match pc.kind {
                        // TABLE-7: a piece ends before a tab; the tab starts the next one (compat <= 14 numbered:
                        // a tab only ends it).
                        Kind::Tab if !numbered => {
                            mn = mn.max(up(word + ind));
                            word = laid[i];
                        }
                        Kind::LineBreak | Kind::PageBreak | Kind::ColumnBreak | Kind::Space | Kind::Tab => {
                            mn = mn.max(up(word + ind));
                            word = 0.0;
                        }
                        _ => {
                            word += pc.width;
                            let joined = old && text_of(pc).starts_with('-') && tail[i + 1];
                            if pc.break_after && !joined {
                                mn = mn.max(up(word + ind));
                                word = 0.0;
                            }
                        }
                    }
                }
                mn = mn.max(up(first.unwrap_or(word) + ind));
                // Each line unwrapped: from its start (the first line's indent included, so a list number in the
                // hanging indent counts) to its last piece that isn't a space; compat <= 14 numbered, the first line
                // from the left edge.
                for (li, l) in lines.iter().enumerate() {
                    let start_x = if li == 0 { start0 } else { s.ind_left };
                    // (Centred or right-aligned in the 1e9pt measuring width, a line starts far right: take that off.)
                    let shift = l.xs.first().map_or(0.0, |&x| (x - start_x).max(0.0));
                    let end = (0..l.xs.len())
                        .rev()
                        .find(|&k| !matches!(pieces.get(l.start + k).map(|q| q.kind), Some(Kind::Space)))
                        .map_or(start_x, |k| l.xs[k] + l.widths[k] - shift);
                    let from = if numbered && li == 0 { 0.0 } else { start_x };
                    let mut w = if old { (end - from).max(0.0) + ind } else { end.max(0.0) + s.ind_right };
                    // TABLE-8: compat 15, a last line starting left of 0 keeps its trailing spaces and the mark that
                    // ends it (the end-of-cell mark in the cell's last paragraph, else the pilcrow).
                    if !old && li + 1 == lines.len() && start_x < 0.0 {
                        let full = l.xs.last().zip(l.widths.last()).map_or(start_x, |(x, w)| x + w - shift);
                        let mark = p.mark_in(env.ctx.env.text.review.view);
                        let glyph = if bi + 1 == blocks.len() { '\u{a4}' } else { '\u{b6}' };
                        let mark_w = env
                            .ctx
                            .fonts
                            .face(&mark.ascii, mark.bold, mark.italic)
                            .and_then(|f| Some(f.advance(f.glyph(glyph)?) * f.scale(mark.size)));
                        w = w.max(full + mark_w.unwrap_or(0.0) + s.ind_right);
                    }
                    mx = mx.max(up(w));
                }
            }
            &LaidBlock::Table(t) => {
                // The text width it takes: its widths less TABLE-17's edge allowances (TABLE-11), or laid out
                // in a cell that wide its A falls short and its words break.
                let t = super::story::as_nested(t, Some(CellEnds { fixed, ..CellEnds::default() }));
                let shown = in_view(&t, env.ctx.env.text.review.view, env.ctx.expansions);
                let t = shown.as_ref().map_or(&*t, |v| &v.table);
                let f = fit(t, 0.0, env, Some(autofit::MAXW), memos)?;
                // TABLE-11: a nested table is never sloppy, so its allowances are compat 15's in every mode.
                let (al, ar) = edge_add(Compat::Generic, t);
                let e = twips(al) + twips(ar) - twips(t.ind);
                mn = mn.max(f.min_t - e);
                mx = mx.max(f.width - e);
                hard = hard.max(f.hard_t - e);
            }
        }
    }
    Ok((mn, mx.max(mn), hard))
}

/// A table's column widths (points, border centre to border centre) and its autofit minimum, width and hard minimum
/// in twips (what a cell holding it needs).
struct Fit {
    cols: Vec<f64>,
    min_t: i64,
    width: i64,
    hard_t: i64,
}

/// Column widths (TABLE-4 to TABLE-21), for fixed and autofit layout. `avail`: A in twips instead of the text width's.
fn fit(t: &Table, text_w: f64, env: &StoryEnv, avail: Option<i64>, memos: &mut Memos) -> Result<Fit, Yield> {
    let edges = cell_edges(t, grid_columns(t), &|_| None);
    let inputs = fit_inputs(t, env, memos, &edges, &|_| None)?;
    Ok(resolve_fit(t, text_w, env, avail, &inputs))
}

/// The grid's column count.
fn grid_columns(t: &Table) -> usize {
    t.grid.len().max(t.rows.iter().map(|r| r.grid_before + r.cells.iter().map(|c| c.span).sum::<usize>() + r.grid_after).max().unwrap_or(0))
}

/// TABLE-7, by row: each cell's min, max and hard minimum, with its padding. `kept`: a row's inputs as they were,
/// when its content didn't change.
fn fit_inputs<'k>(
    t: &Table,
    env: &StoryEnv,
    memos: &mut Memos,
    geo: &[Vec<CellEdges>],
    kept: &dyn Fn(usize) -> Option<&'k FitRow>,
) -> Result<Vec<FitRow>, Yield> {
    let n = grid_columns(t);
    let s2 = twips(2.0 * t.cell_spacing.max(0.0));
    let spacing = twips(t.cell_spacing.max(0.0));
    let grid_tw: Vec<i64> = (0..n).map(|i| twips(t.grid.get(i).copied().unwrap_or(0.0))).collect();
    let mut rows = Vec::with_capacity(t.rows.len());
    for (ri, (r, gs)) in t.rows.iter().zip(geo).enumerate() {
        if let Some(k) = kept(ri).filter(|k| k.cells.len() == gs.len()) {
            rows.push(k.clone());
            continue;
        }
        let mut cells: Vec<autofit::Cell> = Vec::with_capacity(gs.len());
        let mut widths = Vec::new();
        for g in gs {
            let c = &r.cells[g.ci];
            let (bl, br) = (twips(space_h(&g.edges[1])), twips(space_h(&g.edges[3])));
            let (ml, mr) = (twips(c.margins[1]), twips(c.margins[3]));
            let gap = |b: i64, m: i64| if t.cell_spacing > 0.0 { spacing + b + m } else { m };
            let pad = (gap(bl, ml) + gap(br, mr)).max((bl + br + 1) / 2);
            let cell_pad = (pad - 2 * spacing).max(0);
            let continuation = c.v_merge == VMerge::Continue;
            let grid_w: i64 = grid_tw[g.col..g.col + g.span].iter().sum();
            // TABLE-12: a width from the grid less the cell's spacing, half of it more for each side on the table's
            // outer edge, so the resolved columns give the grid back (TABLE-21).
            let from_grid = grid_w - s2 - s2 / 2 * (i64::from(g.col == 0) + i64::from(g.col + g.span >= n));
            let (kind, pref) = match c.width {
                Some(Width::Dxa(w)) => (autofit::Kind::Dxa, twips(w)),
                Some(Width::Pct(f)) => (autofit::Kind::Pct, (f * 5000.0).round() as i64),
                // TABLE-12: w:tcW type="nil": dxa at the cell's grid width less its spacing.
                Some(Width::Nil) => (autofit::Kind::Dxa, from_grid),
                // TABLE-12: fixed layout, an auto cell (or one without w:tcW) takes its grid width.
                Some(Width::Auto) | None if t.fixed => (autofit::Kind::Dxa, from_grid),
                // TABLE-12: no w:tcW reads as auto.
                Some(Width::Auto) | None => (autofit::Kind::Auto, 0),
            };
            let (min, max, hard) = if t.fixed {
                // TABLE-5: fixed layout ignores the content: min = max = padding + 20.
                (cell_pad + 20, cell_pad + 20, cell_pad)
            } else {
                let rotated = matches!(c.text_direction.as_str(), "btLr" | "tbRl" | "tbRlV" | "tbLrV");
                let (mut cmn, mut cmx, chard) = if continuation {
                    (0, 0, 0)
                } else if let (true, Some(Width::Dxa(w))) = (c.fit_text, &c.width) {
                    // TABLE-51: a fitted cell with a dxa width is that wide, whatever its content.
                    let w = (twips(*w) - cell_pad).max(0);
                    (w, w, w)
                } else if rotated {
                    // TABLE-33: as thick as its lines (282 twips for one line of Calibri 11).
                    let w = vertical_width(&c.blocks, env, memos)?;
                    (w, w, 0)
                } else {
                    let w = content_widths(&c.blocks, env, t.fixed, memos)?;
                    widths.push(w.clone());
                    *w
                };
                if (cmn, cmx) == (0, 0) && !continuation {
                    (cmn, cmx) = (6, 6); // TABLE-7: an empty cell
                }
                // (w:noWrap doesn't change the widths, TABLE-10.)
                let min = (cmn + cell_pad).min(autofit::MAXW);
                let max = (cmx + cell_pad).max(min).min(autofit::MAXW);
                (min, max, (chard + cell_pad).min(autofit::MAXW))
            };
            cells.push(autofit::Cell { first: g.col, span: g.span, min, max, hard, kind, pref, row: ri, cont: continuation });
        }
        rows.push(FitRow { cells, widths });
    }
    Ok(rows)
}

/// TABLE-13 to TABLE-19 from the cells' inputs: the column widths and the table's autofit minimum, width and hard
/// minimum.
fn resolve_fit(t: &Table, text_w: f64, env: &StoryEnv, avail: Option<i64>, inputs: &[FitRow]) -> Fit {
    let n = grid_columns(t);
    // TABLE-17: A = text width − indent + the edge allowances.
    let (add_l, add_r) = edge_add(env.ctx.env.text.compat, t);
    let a = avail.unwrap_or_else(|| twips(text_w - t.ind + add_l + add_r));
    let s2 = twips(2.0 * t.cell_spacing.max(0.0));
    let cells: Vec<autofit::Cell> = inputs.iter().flat_map(|r| r.cells.iter().copied()).collect();
    // TABLE-13 to TABLE-19: the columns, then the widths from the table-wide inputs.
    let gaps = skipped(t, n, &cells);
    let cols = autofit::columns(n, &cells, s2, &gaps, t.fixed);
    let pad_total = s2 * (n as i64 + 1);
    let tbl_w = match t.width {
        Width::Dxa(w) => twips(w),
        Width::Pct(p) => autofit::rdiv(a, (p * 5000.0).round() as i64, 5000),
        _ => 0,
    }
    .min(autofit::MAXW - twips(t.ind));
    let no_grow = !env.ctx.env.text.grow_autofit && tbl_w <= a;
    let r = autofit::resolve(&cols, pad_total, tbl_w, a, t.fixed, no_grow);
    let mut widths = r.widths.clone();
    autofit::with_spacing(&mut widths, s2);
    // shortcut: TABLE-21's second pass (an edge reaching MAXW with growAutofit) isn't done; tables 22in wide only.
    Fit { cols: widths.iter().map(|&w| w as f64 / 20.0).collect(), min_t: r.min_t, width: r.width, hard_t: r.hard_t }
}

/// TABLE-15: each row's skipped regions as dxa segments (end column, span, width), counted only inside the extent of
/// the table's non-auto cells; a region without a width is as wide as its grid columns, and the columns a row
/// doesn't reach are an after region.
fn skipped(t: &Table, n: usize, cells: &[autofit::Cell]) -> Vec<Vec<autofit::Gap>> {
    let mut xs = vec![0i64];
    for i in 0..n {
        xs.push(xs[i] + twips(t.grid.get(i).copied().unwrap_or(0.0)));
    }
    let x = |k: usize| xs[k.min(n)];
    let fixed: Vec<&autofit::Cell> = cells.iter().filter(|c| c.kind != autofit::Kind::Auto).collect();
    let (Some(lo), Some(hi)) = (fixed.iter().map(|c| x(c.first)).min(), fixed.iter().map(|c| x(c.first + c.span)).max()) else {
        return Vec::new();
    };
    t.rows
        .iter()
        .map(|r| {
            let mut out = Vec::new();
            let (gb, end) = (r.grid_before, r.grid_before + r.cells.iter().map(|c| c.span).sum::<usize>());
            if gb > 0 && gb <= n && lo < x(gb) {
                out.push((gb - 1, gb, r.w_before.map_or(x(gb) - x(0), twips)));
            }
            if end < n && x(end) < hi {
                let w = if r.grid_after > 0 { r.w_after } else { None };
                out.push((n - 1, n - end, w.map_or(x(n) - x(end), twips)));
            }
            out
        })
        .collect()
}

/// A cell's place in its table: grid column and span, box x and width, text x and width, and its four edges after
/// conflict resolution (TABLE-36).
struct CellGeo {
    ci: usize,
    col: usize,
    span: usize,
    x: f64,
    w: f64,
    cx: f64,
    cw: f64,
    edges: Rc<[Option<Border>; 4]>,
}

/// Lays out a table for a column (or cell) of width `text_w`. `measure`: widths only (no content layout). The column
/// widths and the cells' stories come from their own cache entries; the rows are cached by those results' identity.
/// `prev`: the paragraph before the table (PARA-4).
pub fn layout(t: &Table, text_w: f64, env: &StoryEnv, prev: Option<&ParaStyle>, measure: bool, memos: &mut Memos) -> Result<Rc<TableLayout>, Yield> {
    layout_with(t, text_w, env, prev, measure, None, memos)
}

/// [`layout`] after an edit inside rows `rows`, with `old` the table's layout before it: every other row's
/// cells keep their autofit inputs, and their content where it keeps its width, so a keystroke in a long table
/// doesn't measure and look up every cell again. The columns, geometry and rows are still worked out whole.
pub fn relayout_rows(
    t: &Table,
    text_w: f64,
    env: &StoryEnv,
    prev: Option<&ParaStyle>,
    old: &TableLayout,
    rows: std::ops::RangeInclusive<usize>,
    memos: &mut Memos,
) -> Result<Rc<TableLayout>, Yield> {
    if old.rows.len() != t.rows.len() || old.fit.len() != t.rows.len() {
        return layout(t, text_w, env, prev, false, memos);
    }
    layout_with(t, text_w, env, prev, false, Some((old, rows)), memos)
}

fn layout_with(
    t: &Table,
    text_w: f64,
    env: &StoryEnv,
    prev: Option<&ParaStyle>,
    measure: bool,
    reuse: Option<(&TableLayout, std::ops::RangeInclusive<usize>)>,
    memos: &mut Memos,
) -> Result<Rc<TableLayout>, Yield> {
    match in_view(t, env.ctx.env.text.review.view, env.ctx.expansions) {
        None => layout_shown(t, text_w, env, prev, measure, reuse, memos),
        Some(v) => Ok(v.expand(&*layout_shown(&v.table, text_w, env, prev, measure, None, memos)?)),
    }
}

/// A table as the markup view shows it (REV-1, REV-2): without the rows and cells the view hides, with the grid,
/// vertical merges and table, row and cell properties before their tracked changes in Original. `rows`: per model row,
/// its cells' model indices, or `None` when hidden.
struct ViewTable {
    table: Table,
    rows: Vec<Option<Vec<usize>>>,
}

/// The table `view` shows, when it isn't `t` itself: also when a row shows a sub-template's row in its place
/// (`expansions`).
// shortcut: in Original a cell keeps the formatting its current table style gives it, even when the table's earlier
// properties name another style; resolving the cells again over that style would show it.
fn in_view(t: &Table, view: MarkupView, expansions: Option<&super::expansion::Expansions>) -> Option<ViewTable> {
    let original = view == MarkupView::Original;
    let grid = t.table_grid_in(view);
    let differs = |c: &Cell| view.hides_cell(&c.rev) || c.v_merge_in(view) != c.v_merge || original && c.rev.change.is_some();
    let hides = t.rows.iter().any(|r| view.hides_row(&r.rev) || original && r.rev.change.is_some() || r.cells.iter().any(differs));
    let shown_row = |ri: usize| expansions.and_then(|e| e.rows.get(&(super::ids::TableId(t.id), ri)));
    let expanded = expansions.is_some_and(|e| !e.rows.is_empty()) && (0..t.rows.len()).any(|ri| shown_row(ri).is_some());
    if !hides && !expanded && grid == t.grid.as_slice() && !(original && t.change.is_some()) {
        return None;
    }
    let mut table = t.clone();
    table.grid = grid.to_vec();
    if let Some(p) = t.change.as_ref().and_then(|c| c.previous.as_ref()).filter(|_| original) {
        p.restore_table(&mut table);
    }
    let previous = |c: &Option<std::rc::Rc<crate::docx::review::TableChange>>| c.as_ref().filter(|_| original).and_then(|c| c.previous.clone());
    let mut rows = Vec::with_capacity(t.rows.len());
    table.rows = std::mem::take(&mut table.rows)
        .into_iter()
        .enumerate()
        .filter_map(|(ri, mut r)| {
            // A sub-template's row in place of its reference's: its cells, each kept as the model cell at its place.
            if let Some(body) = shown_row(ri) {
                let n = r.cells.len().max(1);
                r.cells = body.cells.clone();
                rows.push(Some((0..r.cells.len()).map(|k| k.min(n - 1)).collect()));
                return Some(r);
            }
            if view.hides_row(&r.rev) {
                rows.push(None);
                return None;
            }
            let keep: Vec<usize> = (0..r.cells.len()).filter(|&k| !view.hides_cell(&r.cells[k].rev)).collect();
            r.cells = std::mem::take(&mut r.cells).into_iter().filter(|c| !view.hides_cell(&c.rev)).collect();
            if let Some(p) = previous(&r.rev.change) {
                p.restore_row(&mut r);
            }
            for c in &mut r.cells {
                c.v_merge = c.v_merge_in(view);
                if let Some(p) = previous(&c.rev.change) {
                    p.restore_cell(c);
                }
            }
            rows.push(Some(keep));
            Some(r)
        })
        .collect();
    Some(ViewTable { table, rows })
}

impl ViewTable {
    /// The shown table's layout with the model's rows: a hidden row takes no space, and cells keep their model index.
    fn expand(&self, tb: &TableLayout) -> Rc<TableLayout> {
        let (mut shown, mut fit) = (tb.rows.iter().zip(tb.fit.iter()), Vec::with_capacity(self.rows.len()));
        let mut rows = Vec::with_capacity(self.rows.len());
        for keep in &self.rows {
            let Some((keep, (r, f))) = keep.as_ref().and_then(|k| Some((k, shown.next()?))) else {
                let row = RowBox {
                    cells: Vec::new(),
                    top_border: 0.0,
                    height: 0.0,
                    cant_split: false,
                    keep_next: false,
                    break_before: false,
                    header: false,
                    exact: false,
                    min_h: 0.0,
                    gap_top: 0.0,
                    seams: Vec::new(),
                    hidden: true,
                    changed: false,
                };
                rows.push(row);
                fit.push(FitRow::default());
                continue;
            };
            let mut r = r.clone();
            for c in &mut r.cells {
                c.index = keep.get(c.index).copied().unwrap_or(c.index);
            }
            rows.push(r);
            fit.push(f.clone());
        }
        Rc::new(TableLayout {
            x0: tb.x0,
            cols: tb.cols.clone(),
            rows,
            bottom_border: tb.bottom_border,
            spacing: tb.spacing,
            id: tb.id,
            outer: tb.outer.clone(),
            fit: fit.into(),
            rtl: tb.rtl,
        })
    }
}

/// [`layout_with`] for a table as shown.
fn layout_shown(
    t: &Table,
    text_w: f64,
    env: &StoryEnv,
    prev: Option<&ParaStyle>,
    measure: bool,
    reuse: Option<(&TableLayout, std::ops::RangeInclusive<usize>)>,
    memos: &mut Memos,
) -> Result<Rc<TableLayout>, Yield> {
    let kept = |ri: usize| reuse.as_ref().filter(|(_, rows)| !rows.contains(&ri)).map(|(o, _)| *o);
    let n = grid_columns(t);
    // A row's edges depend on its neighbours: those of rows at least one away from the edited ones are as they were.
    let near = |ri: usize| reuse.as_ref().is_some_and(|(_, rows)| ri + 1 >= *rows.start() && ri <= rows.end() + 1);
    let edges = crate::prof::time("table:edges", || cell_edges(t, n, &|ri| reuse.as_ref().filter(|_| !near(ri)).map(|(o, _)| &o.rows[ri])));
    let inputs = crate::prof::time("table:fit", || fit_inputs(t, env, memos, &edges, &|ri| kept(ri).map(|o| &o.fit[ri])))?;
    let Fit { cols, min_t, width, hard_t } = resolve_fit(t, text_w, env, None, &inputs);
    let geo = if cols.len() == n { place(t, &cols, &edges) } else { cell_geometry(t, &cols) };

    let mut stories = Vec::new();
    let mut closings = Vec::new();
    // PARA-4: a cell's neighbours in document order are the previous cell's last paragraph and the next cell's
    // first, the row's end mark (the default paragraph style) and, before the first row, the paragraph before the
    // table. allowSpaceOfSameStyleInTable (compat <= 14) keeps a cell's spacing.
    let contextual = super::paragraph::contextual_in(&env.ctx.env.text, true);
    let mark = Some(&env.ctx.env.text.default_style);
    let view = env.ctx.env.text.review.view;
    fn style(b: Option<&Block>, view: crate::docx::review::MarkupView) -> Option<&Option<String>> {
        match b {
            Some(Block::Para(p)) => Some(&p.para_style_in(view).style_id),
            _ => None,
        }
    }
    let drops = |b: Option<&Block>, other: Option<&Option<String>>| {
        contextual && matches!(b, Some(Block::Para(p)) if super::paragraph::drops_at_edge(p.para_style_in(view), other))
    };
    crate::prof::time("table:cells", || -> Result<(), Yield> {
        for (ri, (r, cells)) in t.rows.iter().zip(&geo).enumerate() {
            let old_row = kept(ri).map(|o| &o.rows[ri]).filter(|o| o.cells.len() == cells.len());
            for (gi, g) in cells.iter().enumerate() {
                let c = &r.cells[g.ci];
                closings.push(closing_para(c));
                let rotated = matches!(c.text_direction.as_str(), "btLr" | "tbRl" | "tbRlV" | "tbLrV");
                let same = old_row.map(|o| &o.cells[gi]).filter(|o| o.index == g.ci && o.cw == g.cw);
                stories.push(if measure || c.v_merge == VMerge::Continue || rotated {
                    None
                } else if let Some(o) = same {
                    Some(o.units.clone())
                } else {
                    // TABLE-26: a cell's closing empty paragraph after a nested table takes no space.
                    let n = c.blocks.len();
                    let skip = closing_para(c).is_some();
                    let blocks: Vec<&Block> = c.blocks[..if skip { n - 1 } else { n }].iter().collect();
                    let before = match g.ci {
                        0 if ri == 0 => prev.map(|p| &p.style_id),
                        0 => mark,
                        ci => style(r.cells[ci - 1].blocks.last(), view),
                    };
                    let after = r.cells.get(g.ci + 1).map_or(mark, |n| style(n.blocks.first(), view));
                    let ends =
                        CellEnds { before: drops(c.blocks.first(), before), after: drops(c.blocks.last(), after), fixed: t.fixed, fit: c.fit_text };
                    // FONT-15: automatic text is white over a dark cell.
                    let shaded;
                    let cell_env = match c.shading.filter(|f| super::env::is_dark(*f)) {
                        Some(fill) => {
                            let text = super::env::TextEnv { backdrop: Some(fill), ..env.ctx.env.text.clone() };
                            shaded = super::LayoutEnv { text, ..env.ctx.env.clone() };
                            StoryEnv { ctx: super::LayoutCtx { env: &shaded, ..env.ctx }, ..*env }
                        }
                        None => *env,
                    };
                    Some(super::story::layout(&blocks, g.cw, &cell_env, Some(ends), memos)?)
                });
            }
        }
        Ok(())
    })?;
    let probe = TableProbe {
        table: t,
        text_w: text_w.to_bits(),
        measure,
        cols: &cols,
        fit: (min_t, width, hard_t),
        cells: &stories,
        text: &env.ctx.env.text,
        fonts: env.ctx.fonts.revision(),
    };
    if !memos.cache.store_tables {
        return Ok(match memos.cache.tables.lookup(&probe) {
            Some(tb) => tb,
            None => Rc::new(assemble(t, text_w, env, cols, &geo, &stories, &closings, inputs.into())),
        });
    }
    cached(memos, |c| &mut c.tables, probe, table_bytes, |_| Ok(assemble(t, text_w, env, cols.clone(), &geo, &stories, &closings, inputs.into())))
}

/// A cell's closing empty paragraph after a nested table, which takes no space (TABLE-26): its id and mark size.
fn closing_para(c: &Cell) -> Option<(ParaId, f64)> {
    match c.blocks.as_slice() {
        [.., Block::Table(_), Block::Para(p)] if p.inlines.is_empty() => Some((ParaId(p.id), p.mark.size)),
        _ => None,
    }
}

/// Whether one of the cell's paragraphs anchors a float laid out in the cell (`layoutInCell`, FLOAT-14).
fn holds_float(c: &Cell) -> bool {
    c.blocks.iter().any(|b| matches!(b, Block::Para(p) if p.inlines.iter().any(|i| matches!(i, Inline::Float(f, _) if f.in_cell(Compat::Legacy)))))
}

/// Each cell's geometry, by row (cells outside the grid are left out).
fn cell_geometry(t: &Table, cols: &[f64]) -> Vec<Vec<CellGeo>> {
    place(t, cols, &cell_edges(t, cols.len(), &|_| None))
}

/// A cell's grid place and resolved edges (TABLE-36): what its geometry is, before the column widths.
struct CellEdges {
    ci: usize,
    col: usize,
    span: usize,
    edges: Rc<[Option<Border>; 4]>,
}

/// Each cell's grid place and edges, by row, for `ncols` grid columns. `kept`: a row as laid out before, when it and
/// its neighbours didn't change (resolving every cell's borders is most of a long table's keystroke).
fn cell_edges<'k>(t: &Table, ncols: usize, kept: &dyn Fn(usize) -> Option<&'k RowBox>) -> Vec<Vec<CellEdges>> {
    let nrows = t.rows.len();
    // Each cell's side borders (cell level beats table level, TABLE-36), borrowed: only an edge's winner is copied.
    struct Side<'a> {
        b: Option<&'a Border>,
        cell: bool,
    }
    fn side<'a>(own: &'a Option<Border>, table: &'a Option<Border>) -> Side<'a> {
        match own {
            Some(b) => Side { b: Some(b), cell: true },
            None => Side { b: table.as_ref(), cell: false },
        }
    }
    let mut grid_cells: Vec<Vec<(usize, usize)>> = Vec::new(); // per row: (col, span) of each cell
    for r in &t.rows {
        let mut col = r.grid_before;
        let mut v = Vec::new();
        for c in &r.cells {
            v.push((col, c.span));
            col += c.span;
        }
        grid_cells.push(v);
    }
    // TABLE-39: with cell spacing each cell keeps its own candidates; neighbours' edges don't meet.
    let alone = t.cell_spacing > 0.0;
    let mut out = Vec::new();
    for (ri, r) in t.rows.iter().enumerate() {
        if let Some(k) = kept(ri).filter(|k| k.cells.iter().all(|c| c.index < r.cells.len())) {
            out.push(k.cells.iter().map(|c| CellEdges { ci: c.index, col: c.col, span: c.span, edges: c.edges.clone() }).collect());
            continue;
        }
        let mut cells = Vec::new();
        for (ci, c) in r.cells.iter().enumerate() {
            let (col, span) = grid_cells[ri][ci];
            let span = span.min(ncols.saturating_sub(col)).max(if col < ncols { 1 } else { 0 });
            if span == 0 {
                continue;
            }
            let top_t = if ri == 0 { &t.borders[0] } else { &t.borders[4] };
            let bot_t = if ri + 1 == nrows { &t.borders[2] } else { &t.borders[4] };
            let left_t = if col == 0 { &t.borders[1] } else { &t.borders[5] };
            let right_t = if col + span >= ncols { &t.borders[3] } else { &t.borders[5] };
            let (top, left, bottom, right) = (side(&c.top, top_t), side(&c.left, left_t), side(&c.bottom, bot_t), side(&c.right, right_t));
            // Neighbours on shared edges.
            let neighbour = |rr: usize, cc: usize| -> Option<&Cell> {
                t.rows.get(rr).and_then(|row| grid_cells[rr].iter().position(|&(c0, s)| cc >= c0 && cc < c0 + s).map(|i| &row.cells[i]))
            };
            let top_e = if ri == 0 || alone {
                conflict((top.b, top.cell), (None, false))
            } else {
                let above = neighbour(ri - 1, col);
                let ab = above.map(|a| side(&a.bottom, &t.borders[4]));
                conflict((top.b, top.cell), ab.map_or((None, false), |s| (s.b, s.cell)))
            };
            let bot_e = if ri + 1 == nrows || alone {
                conflict((bottom.b, bottom.cell), (None, false))
            } else {
                let below = neighbour(ri + 1, col);
                let bl = below.map(|a| side(&a.top, &t.borders[4]));
                conflict((bottom.b, bottom.cell), bl.map_or((None, false), |s| (s.b, s.cell)))
            };
            let left_e = if ci == 0 || alone {
                conflict((left.b, left.cell), (None, false))
            } else {
                let lc = &r.cells[ci - 1];
                let ls = side(&lc.right, &t.borders[5]);
                conflict((left.b, left.cell), (ls.b, ls.cell))
            };
            let right_e = if ci + 1 == r.cells.len() || alone {
                conflict((right.b, right.cell), (None, false))
            } else {
                let rc = &r.cells[ci + 1];
                let rs = side(&rc.left, &t.borders[5]);
                conflict((right.b, right.cell), (rs.b, rs.cell))
            };
            cells.push(CellEdges { ci, col, span, edges: Rc::new([top_e, left_e, bot_e, right_e]) });
        }
        out.push(cells);
    }
    out
}

/// The cells' geometry from their edges and the column widths.
fn place(t: &Table, cols: &[f64], edges: &[Vec<CellEdges>]) -> Vec<Vec<CellGeo>> {
    let col_x: Vec<f64> = std::iter::once(0.0)
        .chain(cols.iter().scan(0.0, |acc, w| {
            *acc += w;
            Some(*acc)
        }))
        .collect();
    let ncols = cols.len();
    edges
        .iter()
        .zip(&t.rows)
        .map(|(row, r)| {
            row.iter()
                .map(|e| {
                    let (c, col, span) = (&r.cells[e.ci], e.col, e.span);
                    let (mut left_e, mut right_e) = (&e.edges[1], &e.edges[3]);
                    let (mut left_outer, mut right_outer) = (col == 0, col + span >= ncols);
                    // TABLE-23: a right-to-left cell's margins stay on their sides as drawn; its borders and spacing
                    // are logical, so they swap.
                    if rtl(t) {
                        (left_e, right_e, left_outer, right_outer) = (right_e, left_e, right_outer, left_outer);
                    }
                    let (x, w) = (col_x[col], col_x[col + span] - col_x[col]);
                    // TABLE-24; with cell spacing, TABLE-39.
                    let (cx, cw) = if t.cell_spacing > 0.0 {
                        let (gl, gr) = (gap(t.cell_spacing, left_outer), gap(t.cell_spacing, right_outer));
                        let cx = gl + space_h(left_e) + c.margins[1];
                        (cx, (w - cx - c.margins[3] - space_h(right_e) - gr).max(0.0))
                    } else {
                        let cx = c.margins[1].max(box_h(left_e) / 2.0);
                        (cx, (w - cx - c.margins[3].max(box_h(right_e) / 2.0)).max(0.0))
                    };
                    CellGeo { ci: e.ci, col, span, x, w, cx, cw, edges: e.edges.clone() }
                })
                .collect()
        })
        .collect()
}

/// The table's rows from its cells' geometry and stories (TABLE-23, TABLE-24, TABLE-27 to TABLE-31, TABLE-39).
fn assemble(
    t: &Table,
    text_w: f64,
    env: &StoryEnv,
    cols: Vec<f64>,
    geo: &[Vec<CellGeo>],
    stories: &[Option<Rc<StoryLayout>>],
    closings: &[Option<(ParaId, f64)>],
    fit: Rc<[FitRow]>,
) -> TableLayout {
    let total: f64 = cols.iter().sum();
    let compat = env.ctx.env.text.compat;
    // TABLE-23: the left border centre.
    let first_left = t.rows.first().and_then(|r| r.cells.first()).map(|c| (space_h(candidate(Some(&c.left), &t.borders[1])), c.margins[1]));
    let (lw, lmar) = first_left.unwrap_or((space_h(&t.borders[1]), 0.0));
    // TABLE-23.
    // TABLE-23: in compat ≤ 14 a table with no tblInd, set directly or by its style, isn't pulled left.
    let mut x0 = match compat {
        Compat::Generic => t.ind + lw / 2.0,
        Compat::Legacy if !t.ind_set => t.ind,
        Compat::Legacy => t.ind - edge_gap(t, lw, lmar),
    };
    match t.jc {
        // A row's empty grid before and after its cells isn't centred, only the cells (the first row's, for the table).
        Jc::Center => {
            let (gb, ga) = t.rows.first().map_or((0, 0), |r| (r.grid_before.min(cols.len()), r.grid_after.min(cols.len())));
            let before: f64 = cols[..gb].iter().sum();
            let after: f64 = cols[cols.len() - ga.min(cols.len() - gb)..].iter().sum();
            x0 = (text_w - (total - before - after)) / 2.0 - before;
        }
        Jc::Right => x0 = text_w - total - space_h(&t.borders[3]) / 2.0,
        _ => {}
    }
    // TABLE-49: a floating table's tblpX takes tblInd's place (compat ≤ 14 adds the tblInd to it); tblpXSpec centres
    // or right-aligns its frame in the column (horzAnchor page: the flow moves it). The frame of an aligned table:
    // compat 15, from the outer box's left to the widest row's last edge plus that cell's right border less its
    // truncated half (no border at all counts a whole 0.75pt), with the box at its left; compat ≤ 14, the tblInd plus the
    // widest row's last-cell content right from the first edge (borders and padding hang outside), the first edge
    // tblInd right of its left.
    if let Some(fp) = t.floating {
        let (e0, ind) = match compat {
            Compat::Generic => (half_border(lw), 0.0),
            Compat::Legacy => (-edge_gap(t, lw, lmar), t.ind),
        };
        let frame = || {
            let row_w = |(r, g): (&Row, &CellGeo)| match compat {
                Compat::Generic => {
                    let table = if g.col + g.span >= cols.len() { &t.borders[3] } else { &t.borders[5] };
                    let b = match (&g.edges[3], candidate(r.cells.get(g.ci).map(|c| &c.right), table)) {
                        (Some(b), _) => thick(b) - ((thick(b) * 20.0).round() as i64 / 2) as f64 / 20.0,
                        (None, None) => 0.75,
                        (None, Some(_)) => 0.0,
                    };
                    g.x + g.w + e0 + b
                }
                Compat::Legacy => g.x + g.cx + g.cw + e0,
            };
            t.rows.iter().zip(geo).filter_map(|(r, gs)| gs.last().map(|g| (r, g))).map(row_w).fold(0.0, f64::max) + ind
        };
        x0 = match fp.x_spec {
            Some(Jc::Center) => (text_w - frame()) / 2.0 + e0 + ind,
            Some(Jc::Right) => text_w - frame() + e0 + ind,
            Some(_) => e0 + ind,
            None => fp.x + e0 + ind,
        };
    }
    // TABLE-23: a right-to-left table is laid out in logical order and drawn mirrored; x0 is its logical last edge.
    let rtl = rtl(t);
    if rtl {
        x0 = rtl_x0(t, total, text_w, rtl_sides(geo.first().into_iter().flatten().map(|g| &*g.edges)), compat == Compat::Legacy);
    }
    let mirror = |x: f64, w: f64| if rtl { total - x - w } else { x };
    let mut stories = stories.iter();
    let mut closings = closings.iter();
    let mut rows = Vec::new();
    let review = &env.ctx.env.text.review;
    for ((ri, r), cells_geo) in t.rows.iter().enumerate().zip(geo) {
        // TABLE-50: in compat ≤ 14 an autofit table's row with its own w:tblPrEx indent moves by the difference; a
        // fixed table's rows all keep the table's.
        // shortcut: left-aligned, non-floating tables only (as measured); the row's width isn't re-solved.
        let shift = match r.ind {
            Some(i) if compat == Compat::Legacy && !t.fixed && t.jc == Jc::Left && t.floating.is_none() && !rtl => i - t.ind,
            _ => 0.0,
        };
        // TABLE-23: in compat ≤ 14 each row of a table that isn't floating is aligned by its own w:jc, else the table's:
        // centred, or right-aligned with its first cell's left margin past the column's right edge, by its own cells'
        // extent with alignTablesRowByRow, else by the whole grid's; a left-aligned one at the left edge allowance.
        // shortcut: a nested table's rows are aligned as a body table's.
        let shift = shift
            + match r.jc.unwrap_or(t.jc) {
                jc @ (Jc::Center | Jc::Right) if compat == Compat::Legacy && t.floating.is_none() && !rtl => {
                    let n = cols.len();
                    let (gb, ga) = if env.ctx.env.text.align_tables_row_by_row { (r.grid_before.min(n), r.grid_after) } else { (0, 0) };
                    let before: f64 = cols[..gb].iter().sum();
                    let after: f64 = cols[n - ga.min(n - gb)..].iter().sum();
                    let width = total - before - after;
                    let gap = r.cells.first().map_or(0.0, |c| c.margins[1])
                        + if t.cell_spacing > 0.0 { 2.0 * t.cell_spacing + space_h(&t.borders[1]) } else { 0.0 };
                    let left = if jc == Jc::Center { (text_w - width) / 2.0 } else { text_w - width + gap };
                    left - (x0 + before)
                }
                // A left-aligned row of a centred or right-aligned table: the table's indent is ignored.
                Jc::Left if compat == Compat::Legacy && t.floating.is_none() && !rtl && t.jc != Jc::Left => -edge_gap(t, lw, lmar) - x0,
                _ => 0.0,
            };
        let mut cells = Vec::new();
        for g in cells_geo {
            let c = &r.cells[g.ci];
            let shading = tint(review, r, c);
            let units = stories.next().cloned().flatten().unwrap_or_default();
            let content_h = units.height;
            // TABLE-30: with doNotVertAlignCellWithSp (compat ≤ 14) a cell holding a float laid out in it isn't
            // aligned; otherwise its floats' bottoms count in the content height it aligns.
            let unaligned = compat == Compat::Legacy && env.ctx.env.text.do_not_vert_align_cell_with_sp && holds_float(c);
            let align_h = content_h.max(units.float_bottom);
            let fixed_floats = if compat == Compat::Legacy { outside_floats(c) } else { Box::default() };
            cells.push(CellBox {
                x: mirror(g.x, g.w) + shift,
                w: g.w,
                cx: g.cx,
                cw: g.cw,
                margins: c.margins,
                units,
                content_h,
                align_h,
                fixed_floats,
                v_align: if unaligned { "top".into() } else { c.v_align.clone() },
                v_merge: c.v_merge,
                col: g.col,
                span: g.span,
                edges: g.edges.clone(),
                shading,
                below: 0.0,
                index: g.ci,
                closing: closings.next().copied().flatten(),
            });
        }
        // REV-4: a revised row or cell marks the row's lines.
        let changed = review.view.change_bars()
            && (r.rev.ins.is_some()
                || r.rev.del.is_some()
                || r.rev.change.is_some()
                || cells_geo.iter().any(|g| r.cells[g.ci].rev.structural.is_some() || r.cells[g.ci].rev.change.is_some()));
        let mut top_border = cells.iter().map(|c| space_h(&c.edges[0])).fold(0.0, f64::max);
        // TABLE-38: a bottom border over part of a wider cell, counted in this row's top border.
        let seams: Vec<(f64, f64, Border)> = match ri.checked_sub(1).and_then(|p| geo.get(p)) {
            Some(above) if t.cell_spacing == 0.0 => above
                .iter()
                .filter(|a| !cells_geo.iter().any(|b| b.col == a.col))
                .filter(|a| {
                    cells_geo.iter().find(|b| a.col >= b.col && a.col < b.col + b.span).is_some_and(|b| r.cells[b.ci].v_merge != VMerge::Continue)
                })
                .filter_map(|a| {
                    let b = a.edges[2].clone().filter(|b| b.visible())?;
                    let (l, r) = if rtl { (&a.edges[3], &a.edges[1]) } else { (&a.edges[1], &a.edges[3]) };
                    let x = mirror(a.x, a.w);
                    Some((x - bw(l) / 2.0, x + a.w + bw(r) / 2.0, b))
                })
                .collect(),
            _ => Vec::new(),
        };
        top_border = seams.iter().map(|s| thick(&s.2)).fold(top_border, f64::max);
        // TABLE-27: the space above the row's content is the same for every cell: the previous row's widest own bottom
        // border (its own, else insideH: the candidate, not the winner), or a cell's own top border plus its top
        // margin when that is more. Each cell's top margin becomes what that leaves below the drawn top border.
        // shortcut: a row first on a later page counts as a middle row.
        if t.cell_spacing == 0.0 {
            let cand = |own: &Option<Border>, table: &Option<Border>| space_h(if own.is_some() { own } else { table });
            let top_t = if ri == 0 { &t.borders[0] } else { &t.borders[4] };
            let w = ri.checked_sub(1).map_or(0.0, |p| t.rows[p].cells.iter().map(|c| cand(&c.bottom, &t.borders[4])).fold(0.0, f64::max));
            let above = cells_geo.iter().map(|g| cand(&r.cells[g.ci].top, top_t) + r.cells[g.ci].margins[0]).fold(w, f64::max);
            for c in &mut cells {
                c.margins[0] = (above - top_border).max(0.0);
            }
        }
        let mut gap_top = 0.0;
        // TABLE-39: with cell spacing each cell draws its own box.
        let mut cell_bottom = 0.0;
        if t.cell_spacing > 0.0 {
            // TABLE-39: 2s above the first row's boxes (and the table's top border), s above the others; s below every
            // row's boxes, 2s below the last's (inside the row: the table's bottom is only its outer border).
            let s = t.cell_spacing;
            gap_top = if ri == 0 { 2.0 * s + space_h(&t.borders[0]) } else { s };
            top_border += gap_top;
            cell_bottom = cells.iter().map(|c| space_h(&c.edges[2])).fold(0.0, f64::max) + gap(s, ri + 1 == t.rows.len());
        }
        // TABLE-27: one bottom margin for the whole row, its cells' largest, below the tallest content.
        let below = cells.iter().map(|c| c.margins[2]).fold(0.0, f64::max);
        let content = cells.iter().filter(|c| c.v_merge == VMerge::None).map(|c| c.margins[0] + c.content_h).fold(f64::NEG_INFINITY, f64::max);
        let content = if content.is_finite() { content + below + cell_bottom } else { 0.0 };
        let min_margins = cells.iter().map(|c| c.margins[0] + c.margins[2]).fold(0.0, f64::max);
        let content = content.max(min_margins);
        // TABLE-8: the cells' bottom margin is outside the row height Word was given, and an atLeast row's top margin
        // too (as Word lays out compat 12 rows; an exact row's top margin is inside it).
        let bottom = cells.iter().map(|c| c.margins[2]).fold(0.0, f64::max);
        let (height, exact, min_h) = match &r.height {
            Some((h, rule)) if rule == "exact" && *h > 0.0 => (*h + bottom, true, 0.0),
            Some((h, rule)) if rule != "auto" => (top_border + content.max(*h + min_margins), false, top_border + *h + min_margins),
            _ => (top_border + content, false, 0.0),
        };
        // The row's first paragraph's style (TABLE-45).
        let first = match r.cells.first().and_then(|c| c.blocks.first()) {
            Some(Block::Para(p)) => Some(p.para_style_in(env.ctx.env.text.review.view)),
            _ => None,
        };
        rows.push(RowBox {
            cells,
            top_border,
            height,
            cant_split: r.cant_split,
            keep_next: first.is_some_and(|s| s.keep_next),
            break_before: first.is_some_and(|s| s.page_break_before),
            header: r.header,
            exact,
            min_h,
            gap_top,
            seams,
            hidden: false,
            changed,
        });
    }
    // TABLE-31: a vertically merged region's extra height goes to its last row.
    for ri in 0..rows.len() {
        for ci in 0..rows[ri].cells.len() {
            if rows[ri].cells[ci].v_merge != VMerge::Restart {
                continue;
            }
            let col = rows[ri].cells[ci].col;
            let mut last = ri;
            while last + 1 < rows.len() && rows[last + 1].cells.iter().any(|c| c.col == col && c.v_merge == VMerge::Continue) {
                last += 1;
            }
            let c = &rows[ri].cells[ci];
            let need = rows[ri].top_border + c.margins[0] + c.content_h + c.margins[2];
            let have: f64 = rows[ri..=last].iter().map(|r| r.height).sum();
            if need > have + EPS && !rows[last].exact {
                rows[last].height += need - have;
            }
            rows[ri].cells[ci].below = rows[ri + 1..=last].iter().map(|r| r.height).sum();
        }
    }
    // TABLE-37: the table's bottom border takes its layout height.
    let mut bottom_border = rows.last().map(|r| r.cells.iter().map(|c| space_h(&c.edges[2])).fold(0.0, f64::max)).unwrap_or(0.0);
    if t.cell_spacing > 0.0 {
        bottom_border = space_h(&t.borders[2]);
    }
    // Drawn sides: a right-to-left table's left border is on its right.
    let (l, r) = if rtl { (3, 1) } else { (1, 3) };
    let outer = [t.borders[0].clone(), t.borders[l].clone(), t.borders[2].clone(), t.borders[r].clone()];
    TableLayout { x0, cols, rows, bottom_border, spacing: t.cell_spacing, outer, id: TableId(t.id), fit, rtl }
}

/// REV-5: in All markup an inserted or deleted row or cell is tinted with its author's colour at a quarter over its
/// shading.
fn tint(review: &ReviewView, r: &Row, c: &Cell) -> Option<u32> {
    let cell =
        c.rev.structural.as_ref().filter(|(_, k)| matches!(k, crate::docx::review::CellMarkKind::Insert | crate::docx::review::CellMarkKind::Delete));
    let Some(m) = cell.map(|(m, _)| m).or(r.rev.del.as_ref()).or(r.rev.ins.as_ref()).filter(|_| review.view == MarkupView::All) else {
        return c.shading;
    };
    let (a, b) = (review.palette.color(m.author), c.shading.unwrap_or(0xFFFFFF));
    let mix = |s: u32| ((((a >> s) & 0xFF) + 3 * ((b >> s) & 0xFF)) / 4) << s;
    Some(mix(16) | mix(8) | mix(0))
}

/// A cell's units to draw: a slice of its story's units, and the top they are rebased to 0 from (a split row's part).
#[derive(Clone, Copy)]
pub struct CellPart<'a> {
    pub units: &'a [StoryUnit],
    pub base: f64,
    /// The part reaches the end of the cell's content (the last part of a split row, or a whole cell).
    pub ends: bool,
}

impl CellPart<'_> {
    /// The part's height: its last unit's bottom, rebased.
    pub fn height(&self) -> f64 {
        self.units.last().map_or(0.0, |l| (l.top - self.base) + l.height)
    }
}

/// The whole of every cell's content (an unsplit row).
pub fn whole(row: &RowBox) -> Vec<CellPart<'_>> {
    row.cells.iter().map(|c| CellPart { units: &c.units.units, base: 0.0, ends: true }).collect()
}

/// The floats anchored in the cell's paragraphs that compat ≤ 14 lays out outside the cell (FLOAT-14).
fn outside_floats(c: &Cell) -> Box<[u64]> {
    let paras = c.blocks.iter().filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None });
    paras
        .flat_map(|p| p.inlines.iter())
        .filter_map(|i| match i {
            Inline::Float(f, _) if !f.in_cell(Compat::Legacy) => Some(f.id),
            _ => None,
        })
        .collect()
}

/// FLOAT-14: the cell's vertical alignment (TABLE-30) doesn't move the floats in `fixed` (compat ≤ 14's
/// layoutInCell="0" ones): they stay where a top-aligned cell puts them, `off` above the aligned content.
fn unaligned_floats(children: &Rc<[Child]>, off: f64, fixed: &[u64]) -> Option<Rc<[Child]>> {
    let float = |c: &Child| matches!(c, Child::Box(b) if matches!(b.kind, BoxKind::Float { float, .. } if fixed.contains(&float.0)));
    if off == 0.0 || !children.iter().any(float) {
        return None;
    }
    let up = |c: &Child| match c {
        Child::Box(b) if float(c) => match b.kind {
            BoxKind::Float { anchor, float, rect, layer } => Child::Box(super::fragment::BoxFragment {
                kind: BoxKind::Float { anchor, float, layer, rect: [rect[0], rect[1] - off, rect[2], rect[3] - off] },
                offset: (b.offset.0, b.offset.1 - off),
                ..b.clone()
            }),
            _ => c.clone(),
        },
        _ => c.clone(),
    };
    Some(children.iter().map(up).collect())
}

/// A cell unit's lines clipped to the cell's box `lo..hi` (x in the unit's coordinates, TABLE-35). None when nothing
/// is outside.
// shortcut: whole glyphs are kept or dropped here; cutting a partly visible glyph at the edge is the painter's job
// (a clip rectangle per cell), not done yet.
pub(super) fn clip_x(children: &Rc<[Child]>, lo: f64, hi: f64, bottom: f64) -> Option<Rc<[Child]>> {
    let inside = |r: &GlyphRun, g: &(u32, f64), dx: f64, dy: f64| {
        let k = r.size / r.face.upem;
        // (Below an exact row's bottom, TABLE-34: the glyph's ink top must reach above it.)
        if bottom.is_finite() && r.face.ink_top(g.0).is_some_and(|t| dy + r.baseline - t * k >= bottom - EPS) {
            return false;
        }
        match r.face.ink(g.0) {
            Some((bearing, width)) => {
                let x0 = dx + g.1 + bearing * k;
                x0 < hi - EPS && x0 + width * k > lo + EPS
            }
            None => (lo..hi).contains(&(dx + g.1)),
        }
    };
    let spills = |l: &PlacedLine| {
        l.line.items.iter().any(|i| match i {
            Item::Glyphs(r) => r.glyphs.iter().any(|g| !inside(r, g, l.x, l.y)),
            Item::Rect { rect, .. } => l.x + rect[0] < lo - EPS || l.x + rect[2] > hi + EPS,
            _ => false,
        })
    };
    // TABLE-35: a nested table isn't clipped to this cell's box, only its exact rows' bottom.
    let nested: Vec<Option<Child>> = children.iter().map(|c| clip_nested(c, lo, hi, bottom)).collect();
    if !children.iter().any(|c| matches!(c, Child::Line { placed, .. } if spills(placed))) && nested.iter().all(Option::is_none) {
        return None;
    }
    let clipped = children.iter().zip(nested).map(|(c, n)| match (c, n) {
        (_, Some(n)) => n,
        (Child::Line { para, placed }, None) if spills(placed) => {
            let (dx, dy) = (placed.x, placed.y);
            // Characters (at their clusters' origins) follow the glyphs kept.
            let items: Vec<Item> = placed
                .line
                .items
                .iter()
                .filter_map(|i| match i {
                    Item::Glyphs(r) => {
                        let glyphs: Box<[(u32, f64)]> = r.glyphs.iter().copied().filter(|g| inside(r, g, dx, dy)).collect();
                        let (from, to) = glyphs.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), g| (a.min(g.1), b.max(g.1)));
                        // A glyph dropped between kept ones (below the clip line) takes its characters with it.
                        let at = |g: &(u32, f64), x: f64| (g.1 - x).abs() < EPS;
                        let dropped = |x: f64| r.glyphs.iter().any(|g| at(g, x) && !inside(r, g, dx, dy)) && !glyphs.iter().any(|g| at(g, x));
                        let chars: Box<[(char, f64, Option<Unit>)]> =
                            r.chars.iter().copied().filter(|ch| ch.1 >= from - EPS && ch.1 <= to + EPS && !dropped(ch.1)).collect();
                        (!glyphs.is_empty() || r.glyphs.is_empty()).then(|| Item::Glyphs(GlyphRun { glyphs, chars, ..r.clone() }))
                    }
                    Item::Rect { rect, colour } => {
                        let (x0, x1) = (rect[0].max(lo - dx), rect[2].min(hi - dx));
                        (x1 > x0).then_some(Item::rect([x0, rect[1], x1, rect[3]], *colour))
                    }
                    other => Some(other.clone()),
                })
                .collect();
            let atoms = placed.line.atoms.iter().copied().filter(|a| (lo..hi).contains(&(dx + a.x0))).collect();
            let line = Rc::new(LineFragment {
                items: items.into_boxed_slice(),
                pitch: placed.line.pitch,
                baseline: placed.line.baseline,
                atoms,
                changed: placed.line.changed,
                mark: placed.line.mark,
                texts: placed.line.texts.clone(),
                expansions: placed.line.expansions.clone(),
            });
            Child::Line { para: *para, placed: PlacedLine { line, ..placed.clone() } }
        }
        (other, None) => other.clone(),
    });
    Some(clipped.collect())
}

/// TABLE-35: a group's content clipped to an enclosing cell's box `lo..hi` (`clip_x`), a nested table's only to the
/// `bottom` of an exact row; None when nothing is outside.
fn clip_nested(c: &Child, lo: f64, hi: f64, bottom: f64) -> Option<Child> {
    match c {
        Child::Group { offset, children } => {
            clip_x(children, lo - offset.0, hi - offset.0, bottom - offset.1).map(|children| Child::Group { offset: *offset, children })
        }
        Child::Table(t) => {
            let mut any = false;
            let rows = t.rows.iter().map(|r| {
                let cells = r.cells.iter().map(|cell| {
                    let content = cell
                        .content
                        .iter()
                        .map(|c| clip_nested(c, f64::NEG_INFINITY, f64::INFINITY, bottom).inspect(|_| any = true).unwrap_or_else(|| c.clone()));
                    CellFragment { content: content.collect(), ..cell.clone() }
                });
                RowFragment { cells: cells.collect(), ..r.clone() }
            });
            let t = TableFragment { table: t.table, rows: rows.collect() };
            any.then_some(Child::Table(t))
        }
        _ => None,
    }
}

/// A cell's edges as drawn: top, left, bottom, right (TABLE-23: a right-to-left cell's logical left is on its right).
fn drawn_edges<'a>(tb: &TableLayout, c: &'a CellBox) -> [&'a Option<Border>; 4] {
    let [t, l, b, r] = &*c.edges;
    if tb.rtl { [t, r, b, l] } else { [t, l, b, r] }
}

/// Draws row `ri` with its top at `top` and the column's left edge at `left`; `parts`: the units of each cell to draw
/// (a row broken across pages), else all of them.
pub fn draw_row(tb: &TableLayout, ri: usize, left: f64, top: f64, height: f64, parts: Option<&[CellPart]>, last_of_table: bool) -> RowFragment {
    let row = &tb.rows[ri];
    let x0 = left + tb.x0;
    let (mut background, mut cells, mut borders) = (Vec::new(), Vec::new(), Vec::new());
    // The row's place for table handles: its extent and the column borders crossing it.
    let mut edges: Vec<(f64, usize)> = Vec::new();
    let mut rect = [x0, top, x0, top + height];
    if let (Some(mut first), Some(mut end)) = (row.cells.first(), row.cells.last()) {
        // TABLE-23: a right-to-left row's first cell is on its right.
        if tb.rtl {
            (first, end) = (end, first);
        }
        // Left to right as drawn.
        let mut order: Vec<&CellBox> = row.cells.iter().collect();
        if tb.rtl {
            order.reverse();
        }
        for c in order {
            let (l, r) = if tb.rtl { (c.col + c.span, c.col) } else { (c.col, c.col + c.span) };
            for e in [(x0 + c.x, l), (x0 + c.x + c.w, r)] {
                if !edges.iter().any(|x| x.1 == e.1) {
                    edges.push(e);
                }
            }
        }
        rect = [x0 + first.x, top, x0 + end.x + end.w, top + height];
    }
    for (k, c) in row.cells.iter().enumerate() {
        let part = parts.map(|p| p[k]).unwrap_or(CellPart { units: &c.units.units, base: 0.0, ends: true });
        // Merged regions: the content sits in the first cell, over the whole region (TABLE-32). TABLE-30: the cell's
        // content height as the row measured it (its last space-after included) when the cell is whole; a part of a
        // split row, its lines.
        // shortcut: a part of a split row aligns its lines only, its floats aside.
        let ch = if part.ends && part.base == 0.0 && part.units.len() == c.units.units.len() { c.align_h } else { part.height() };
        // A merge's content, alignment, clip, shading and side borders cover the whole region.
        let height = height + c.below;
        let inner = height - row.top_border - c.margins[0] - c.margins[2];
        let off = match c.v_align.as_str() {
            "center" => ((inner - ch) / 2.0).max(0.0),
            "bottom" => (inner - ch).max(0.0),
            _ => 0.0,
        };
        let base_y = top + row.top_border + c.margins[0] + off;
        let base_x = x0 + c.x + c.cx;
        // TABLE-34: an exact row clips at its exact height, above the cells' bottom margin the row adds below it.
        // shortcut: a region is clipped at its rows' total when its own row is exact, less this row's bottom margin.
        let below_margin = row.cells.iter().map(|c| c.margins[2]).fold(0.0, f64::max);
        let clip = if row.exact { top + height - below_margin } else { f64::INFINITY };
        let mut content = Vec::new();
        let mut hidden = Vec::new();
        for u in part.units {
            let ut = u.top - part.base;
            // TABLE-28, TABLE-34: an exact row clips its content per glyph.
            let bottom = clip - (base_y + ut) + INK_SLACK;
            let hide = match ink_top(&u.content) {
                Some(top) => top >= bottom - EPS,
                None => base_y + ut >= clip - EPS,
            };
            if hide {
                source_units(&u.content, &mut hidden);
                continue;
            }
            // Text past the cell's box (a negative indent) is clipped at its edges, in the content's coordinates.
            let children = clip_x(&u.content, -c.cx, c.w - c.cx, bottom).unwrap_or_else(|| u.content.clone());
            let children = unaligned_floats(&children, off, &c.fixed_floats).unwrap_or(children);
            content.push(Child::Group { offset: (base_x, base_y + ut), children });
        }
        // The closing paragraph's caret place: below the cell's last content, on the part that shows it.
        let closing = c.closing.filter(|_| part.ends).map(|(para, size)| (para, [base_x, base_y + ch, size * 1.2]));
        let place = CellPlace {
            index: c.index,
            col: c.col,
            span: c.span,
            rect: [x0 + c.x, top, x0 + c.x + c.w, top + height],
            clip: row.exact.then_some(clip),
            v_merge: c.v_merge,
            hidden: hidden.into_boxed_slice(),
            closing,
        };
        cells.push(CellFragment { cell: k, content: content.into_boxed_slice(), place });
        if tb.spacing > 0.0 {
            // TABLE-39: the cell's own box, its borders inside it.
            let ncols = tb.cols.len();
            let (mut lo, mut ro) = (c.col == 0, c.col + c.span >= ncols);
            if tb.rtl {
                (lo, ro) = (ro, lo);
            }
            let bx0 = x0 + c.x + gap(tb.spacing, lo);
            let bx1 = x0 + c.x + c.w - gap(tb.spacing, ro);
            // TABLE-39: a cell box ends the spacing above its row's bottom (2s on the table's last row).
            let (by0, by1) = (top + row.gap_top, top + height - gap(tb.spacing, tb.is_last(ri)));
            if let Some(fill) = c.shading {
                background.push(Item::rect([bx0, by0, bx1, by1], fill));
            }
            let [t, l, b, r] = drawn_edges(tb, c);
            let vis = |e: &'_ Option<Border>| e.as_ref().filter(|b| b.visible()).map(|b| (thick(b), b.clone()));
            if let Some((w, e)) = vis(t) {
                edge(&mut borders, [bx0, by0, bx1, by0 + w], true, &e);
            }
            if let Some((w, e)) = vis(b) {
                edge(&mut borders, [bx0, by1 - w, bx1, by1], true, &e);
            }
            if let Some((w, e)) = vis(l) {
                edge(&mut borders, [bx0, by0, bx0 + w, by1], false, &e);
            }
            if let Some((w, e)) = vis(r) {
                edge(&mut borders, [bx1 - w, by0, bx1, by1], false, &e);
            }
            continue;
        }
        if let Some(fill) = c.shading {
            background.push(Item::rect([x0 + c.x, top, x0 + c.x + c.w, top + height], fill));
        }
        // Borders as filled rectangles centred on the cell edges; edges without one are gridlines (screen only).
        let [t, l, b, r] = drawn_edges(tb, c);
        let (cx0, cx1) = (x0 + c.x, x0 + c.x + c.w);
        let shown = |e: &Option<Border>| e.as_ref().is_some_and(|b| b.visible());
        if !shown(t) && c.v_merge != VMerge::Continue {
            borders.push(Item::Gridline { line: [cx0, top, cx1, top] });
        }
        if !shown(l) {
            borders.push(Item::Gridline { line: [cx0, top, cx0, top + height] });
        }
        if !shown(r) {
            borders.push(Item::Gridline { line: [cx1, top, cx1, top + height] });
        }
        if last_of_table && !shown(b) {
            borders.push(Item::Gridline { line: [cx0, top + height, cx1, top + height] });
        }
        // Inside a merge there is no edge between its rows.
        if let Some(t) = t.as_ref().filter(|b| b.visible() && c.v_merge != VMerge::Continue) {
            edge(&mut borders, [cx0 - bw(l) / 2.0, top, cx1 + bw(r) / 2.0, top + thick(t)], true, t);
        }
        if let Some(l) = l.as_ref().filter(|b| b.visible()) {
            edge(&mut borders, [cx0 - thick(l) / 2.0, top, cx0 + thick(l) / 2.0, top + height], false, l);
        }
        if let Some(r) = r.as_ref().filter(|b| b.visible()) {
            edge(&mut borders, [cx1 - thick(r) / 2.0, top, cx1 + thick(r) / 2.0, top + height], false, r);
        }
        if last_of_table && let Some(b) = b.as_ref().filter(|b| b.visible()) {
            edge(&mut borders, [cx0 - bw(l) / 2.0, top + height, cx1 + bw(r) / 2.0, top + height + thick(b)], true, b);
        }
    }
    for (a, b, s) in &row.seams {
        edge(&mut borders, [x0 + a, top, x0 + b, top + thick(s)], true, s);
    }
    if tb.spacing > 0.0 {
        // The table's own outer border, drawn as usual around the spaced cells (junction ticks not drawn).
        let [t, l, b, r] = &tb.outer;
        let x1 = x0 + tb.cols.iter().sum::<f64>();
        let (lw, rw) = (bw(l), bw(r));
        let bottom = top + height + if last_of_table { tb.bottom_border - bw(b) } else { 0.0 };
        if ri == 0
            && let Some(t) = t.as_ref().filter(|b| b.visible())
        {
            edge(&mut borders, [x0 - lw / 2.0, top, x1 + rw / 2.0, top + thick(t)], true, t);
        }
        if let Some(l) = l.as_ref().filter(|b| b.visible()) {
            edge(&mut borders, [x0 - lw / 2.0, top, x0 + lw / 2.0, bottom], false, l);
        }
        if let Some(r) = r.as_ref().filter(|b| b.visible()) {
            edge(&mut borders, [x1 - rw / 2.0, top, x1 + rw / 2.0, bottom], false, r);
        }
        if last_of_table && let Some(b) = b.as_ref().filter(|b| b.visible()) {
            edge(&mut borders, [x0 - lw / 2.0, bottom, x1 + rw / 2.0, bottom + thick(b)], true, b);
        }
    }
    RowFragment {
        row: ri,
        rect,
        top,
        edges: edges.into_boxed_slice(),
        background: background.into_boxed_slice(),
        cells: cells.into_boxed_slice(),
        borders: borders.into_boxed_slice(),
        repeat: false,
        changed: row.changed,
    }
}

/// A whole row drawn ([`draw_whole_row`]). Compared only by the cache's verify mode, through its debug form (the
/// fragment types have no equality).
#[derive(Debug)]
pub struct DrawnRow(RowFragment);

impl PartialEq for DrawnRow {
    fn eq(&self, o: &Self) -> bool {
        format!("{:?}", self.0) == format!("{:?}", o.0)
    }
}

/// What drawing a whole row reads: the row (its cells' content by identity), its left edge and height (it is drawn at
/// the top 0), whether it ends the table, and the table's left edge, cell spacing, columns and outer borders.
#[derive(Debug)]
pub struct RowKey {
    row: RowBox,
    ri: usize,
    at: [u64; 2],
    last: bool,
    x0: u64,
    spacing: u64,
    cols: Vec<f64>,
    outer: [Option<Border>; 4],
    bottom_border: u64,
    rtl: bool,
}

/// A [`RowKey`] borrowed.
pub struct RowProbe<'a> {
    tb: &'a TableLayout,
    ri: usize,
    at: [u64; 2],
    last: bool,
}

/// Rows equal for drawing: their cells' content the same layouts, everything else equal.
fn same_row(a: &RowBox, b: &RowBox) -> bool {
    a.top_border == b.top_border
        && a.changed == b.changed
        && a.exact == b.exact
        && a.gap_top == b.gap_top
        && a.seams == b.seams
        && a.cells.len() == b.cells.len()
        && a.cells.iter().zip(&b.cells).all(|(x, y)| {
            Rc::ptr_eq(&x.units, &y.units)
                && (x.x, x.w, x.cx, x.cw, x.content_h, x.align_h, x.below) == (y.x, y.w, y.cx, y.cw, y.content_h, y.align_h, y.below)
                && x.margins == y.margins
                && x.v_align == y.v_align
                && x.v_merge == y.v_merge
                && (x.col, x.span, x.index) == (y.col, y.span, y.index)
                && x.edges == y.edges
                && x.shading == y.shading
                && x.closing == y.closing
        })
}

impl CacheKey for RowKey {
    type Probe<'a> = RowProbe<'a>;
    const PIECE: Option<crate::prof::Piece> = Some(crate::prof::Piece::Row);

    fn bucket(p: &RowProbe) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = super::cache::KeyHasher::default();
        (p.tb.id, p.ri, p.at, p.last).hash(&mut h);
        for c in &p.tb.rows[p.ri].cells {
            Rc::as_ptr(&c.units).hash(&mut h);
        }
        h.finish()
    }

    fn matches(&self, p: &RowProbe) -> bool {
        let tb = p.tb;
        self.ri == p.ri
            && self.at == p.at
            && self.last == p.last
            && self.x0 == tb.x0.to_bits()
            && self.spacing == tb.spacing.to_bits()
            && self.bottom_border == tb.bottom_border.to_bits()
            && self.cols == tb.cols
            && self.outer == tb.outer
            && self.rtl == tb.rtl
            && same_row(&self.row, &tb.rows[p.ri])
    }

    fn from_probe(p: &RowProbe) -> RowKey {
        let tb = p.tb;
        RowKey {
            row: tb.rows[p.ri].clone(),
            ri: p.ri,
            at: p.at,
            last: p.last,
            x0: tb.x0.to_bits(),
            spacing: tb.spacing.to_bits(),
            cols: tb.cols.clone(),
            outer: tb.outer.clone(),
            bottom_border: tb.bottom_border.to_bits(),
            rtl: tb.rtl,
        }
    }
}

/// [`draw_row`] for a whole row, through the cache: a page step draws its rows again, and while typing most
/// are where they were or only moved down. Drawn at 0 and moved to `top`, so a moved row is a hit.
pub fn draw_whole_row(
    tb: &TableLayout,
    ri: usize,
    left: f64,
    top: f64,
    height: f64,
    last_of_table: bool,
    memos: &mut Memos,
) -> (RowFragment, Option<Rc<DrawnRow>>) {
    let probe = RowProbe { tb, ri, at: [left.to_bits(), height.to_bits()], last: last_of_table };
    let bytes = |d: &DrawnRow| {
        std::mem::size_of::<RowFragment>()
            + d.0.cells.len() * std::mem::size_of::<CellFragment>()
            + (d.0.borders.len() + d.0.background.len()) * std::mem::size_of::<Item>()
    };
    match cached(memos, |c| &mut c.rows, probe, bytes, |_| Ok(DrawnRow(draw_row(tb, ri, left, 0.0, height, None, last_of_table)))) {
        Ok(d) => (super::page::row_down(&d.0, top), Some(d)),
        Err(_) => (super::page::row_down(&draw_row(tb, ri, left, 0.0, height, None, last_of_table), top), None),
    }
}

/// A laid-out table's rows as story units, and their height.
pub fn table_units(tb: &Rc<TableLayout>) -> (Vec<StoryUnit>, f64) {
    let mut y = 0.0;
    let mut units = Vec::new();
    let n = tb.rows.len();
    for ri in (0..n).filter(|&ri| !tb.rows[ri].hidden) {
        let h = tb.rows[ri].height;
        let last = tb.is_last(ri);
        let row = draw_row(tb, ri, 0.0, 0.0, h, None, last);
        let extra = if last { tb.bottom_border } else { 0.0 };
        let notes: Vec<(NoteKind, i64)> = tb.rows[ri].cells.iter().flat_map(|c| c.units.units.iter().flat_map(|u| u.notes.iter().copied())).collect();
        let fragment = Child::Table(TableFragment { table: tb.id, rows: Box::new([row]) });
        units.push(StoryUnit {
            top: y,
            height: h + extra,
            below: 0.0,
            after: 0.0,
            content: Rc::from([fragment]),
            block: 0,
            widow: false,
            keep_next: false,
            keep_lines: false,
            notes: notes.into_boxed_slice(),
            row: Some(Rc::new(NestedRow { table: tb.clone(), ri, shift: 0.0, rest: None })),
        });
        y += h + extra;
    }
    (units, y)
}

/// Exact rows: slack below the row bottom for a glyph's ink top (1 twip, TABLE-34).
pub(super) const INK_SLACK: f64 = 0.06;

/// The highest ink top among a unit's glyphs, in its coordinates.
fn ink_top(children: &[Child]) -> Option<f64> {
    let line = |dy: f64, items: &[Item]| {
        items
            .iter()
            .flat_map(|it| if let Item::Glyphs(r) = it { r.glyphs.iter().map(|g| (r, g.0)).collect::<Vec<_>>() } else { Vec::new() })
            .filter_map(|(r, g)| r.face.ink_top(g).map(|t| dy + r.baseline - t * r.face.scale(r.size)))
            .reduce(f64::min)
    };
    children
        .iter()
        .filter_map(|c| match c {
            Child::Line { placed, .. } => line(placed.y, &placed.line.items),
            Child::Para(p) => p.lines.iter().filter_map(|l| line(l.y, &l.line.items)).reduce(f64::min),
            Child::Group { offset, children } => ink_top(children).map(|t| t + offset.1),
            _ => None,
        })
        .reduce(f64::min)
}

/// The source units drawn in `children`, by paragraph: (paragraph, first unit, end unit) per glyph run.
fn source_units(children: &[Child], out: &mut Vec<(ParaId, Unit, Unit)>) {
    for c in children {
        match c {
            Child::Para(p) => p.lines.iter().for_each(|l| line_units(&l.line.items, Some(p.para), out)),
            Child::Line { para, placed } => line_units(&placed.line.items, *para, out),
            Child::Group { children, .. } => source_units(children, out),
            Child::Box(b) if !matches!(b.kind, BoxKind::Float { .. }) => source_units(&b.children, out),
            Child::Box(_) => {}
            Child::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|cell| source_units(&cell.content, out)),
            Child::Items(_) => {}
        }
    }
}

fn line_units(items: &[Item], para: Option<ParaId>, out: &mut Vec<(ParaId, Unit, Unit)>) {
    for it in items {
        if let Item::Glyphs(r) = it
            && let Some(para) = r.para.or(para)
        {
            let mut units = r.chars.iter().filter_map(|c| c.2);
            if let Some(first) = units.next() {
                let (lo, hi) = units.fold((first, first), |(lo, hi), u| (lo.min(u), hi.max(u)));
                out.push((para, lo, Unit(hi.0 + 1)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dotted_border_is_drawn_as_dots() {
        // TABLE-37: a 1pt dotted line 9pt long: dots of 1pt every 2pt, the last one at 8.
        let b = Border { style: "dotted".into(), width: 1.0, space: 0.0, shadow: false, color: None };
        let mut out = Vec::new();
        edge(&mut out, [0.0, 0.0, 9.0, 1.0], true, &b);
        let xs: Vec<f64> = out.iter().map(|i| if let Item::Rect { rect, .. } = i { rect[0] } else { -1.0 }).collect();
        assert_eq!(xs, vec![0.0, 2.0, 4.0, 6.0, 8.0]);
    }

    #[test]
    fn a_3d_border_is_three_lines_in_light_middle_and_dark() {
        // TABLE-37: a 1pt black threeDEmboss: 0.75 light, 1 middle, 0.75 dark from the top; engraved the other way.
        let b = |style: &str| Border { style: style.into(), width: 1.0, space: 0.0, shadow: false, color: Some(0) };
        let drawn = |style: &str| {
            let mut out = Vec::new();
            edge(&mut out, [0.0, 0.0, 10.0, 2.5], true, &b(style));
            out.iter().map(|i| if let Item::Rect { rect, colour, .. } = i { (rect[3] - rect[1], *colour) } else { (0.0, 0) }).collect::<Vec<_>>()
        };
        assert_eq!(drawn("threeDEmboss"), vec![(0.75, 0xC0C0C0), (1.0, 0x808080), (0.75, 0x000000)]);
        assert_eq!(drawn("threeDEngrave"), vec![(0.75, 0x000000), (1.0, 0x808080), (0.75, 0xC0C0C0)]);
        // A grey's lines: HLS round trip of 808080 is exact.
        assert_eq!(from_hls(to_hls(0x808080).0, to_hls(0x808080).1, to_hls(0x808080).2), 0x808080);
    }
}
