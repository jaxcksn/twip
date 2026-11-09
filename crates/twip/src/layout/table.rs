//! Table layout

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

// Table cache
#[derive(Debug)]
pub struct TableKey {
    shell: Table,
    text_w: u64,
    measure: bool,
    cols: Vec<f64>,
    fit: (i64, i64, i64),
    cells: Vec<Option<Rc<StoryLayout>>>,
    text: TextEnv,
    fonts: u64,
}

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
            let Row {
                height,
                cant_split,
                header,
                grid_before,
                grid_after,
                w_before,
                w_after,
                ind,
                jc,
                div,
                borders,
                shading,
                vanished,
                cell_spacing: row_spacing,
                run,
                cells,
                src: _,
                controls: _,
                rev,
            } = r;
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
                        hide_mark,
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
                        hide_mark: *hide_mark,
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
                div: div.clone(),
                borders: borders.clone(),
                shading: *shading,
                vanished: *vanished,
                cell_spacing: *row_spacing,
                run: *run,
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
            let Row {
                height,
                cant_split,
                header,
                grid_before,
                grid_after,
                w_before,
                w_after,
                ind,
                jc,
                div,
                borders,
                shading,
                vanished,
                cell_spacing: row_spacing,
                run,
                cells,
                src: _,
                controls: _,
                rev,
            } = r;
            *height == q.height
                && *cant_split == q.cant_split
                && *header == q.header
                && *grid_before == q.grid_before
                && *grid_after == q.grid_after
                && *w_before == q.w_before
                && *w_after == q.w_after
                && *ind == q.ind
                && *jc == q.jc
                && *div == q.div
                && *borders == q.borders
                && *shading == q.shading
                && *vanished == q.vanished
                && *row_spacing == q.cell_spacing
                && *run == q.run
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
                        hide_mark,
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
                        && *hide_mark == d.hide_mark
                        && *rev == d.rev
                })
        })
}

fn table_bytes(t: &TableLayout) -> usize {
    std::mem::size_of::<TableLayout>()
        + t.rows.iter().map(|r| std::mem::size_of::<RowBox>() + r.cells.len() * std::mem::size_of::<CellBox>()).sum::<usize>()
        + t.fit.iter().map(|r| r.cells.len() * std::mem::size_of::<autofit::Cell>()).sum::<usize>()
}

#[derive(Clone, Debug, PartialEq)]
pub struct CellBox {
    pub x: f64,
    pub w: f64,
    pub cx: f64,
    pub cw: f64,
    pub margins: [f64; 4],
    pub units: Rc<StoryLayout>,
    pub content_h: f64,
    pub align_h: f64,
    pub fixed_floats: Box<[u64]>,
    pub v_align: String,
    pub v_merge: VMerge,
    pub col: usize,
    pub span: usize,
    pub edges: Rc<[Option<Border>; 4]>,
    pub shading: Option<u32>,
    pub below: f64,
    pub index: usize,
    pub closing: Option<(ParaId, f64)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RowBox {
    pub cells: Vec<CellBox>,
    pub top_border: f64,
    pub height: f64,
    pub cant_split: bool,
    pub keep_next: bool,
    pub break_before: bool,
    pub header: bool,
    pub exact: bool,
    pub min_h: f64,
    pub gap_top: f64,
    pub seams: Vec<(f64, f64, Border)>,
    pub hidden: bool,
    pub changed: bool,
    /// The row's HTML division: room above and below it, and its boxes.
    pub lead: f64,
    pub trail: f64,
    pub div_boxes: Rc<[crate::docx::model::DivBox]>,
    pub shading: Option<u32>,
    /// The row's cell spacing; its cells are framed when it has one.
    pub spacing: Option<f64>,
    /// The next row is framed: the row draws its own bottom border.
    pub closes: bool,
}

impl TableLayout {
    pub fn is_last(&self, ri: usize) -> bool {
        self.rows.get(ri + 1..).is_none_or(|rest| rest.iter().all(|r| r.hidden))
    }
}

#[derive(Debug, PartialEq)]
pub struct TableLayout {
    pub x0: f64,
    pub cols: Vec<f64>,
    pub rows: Vec<RowBox>,
    pub bottom_border: f64,
    pub spacing: f64,
    pub id: TableId,
    pub outer: [Option<Border>; 4],
    pub fit: Rc<[FitRow]>,
    pub rtl: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct FitRow {
    cells: Vec<autofit::Cell>,
    widths: Vec<Rc<(i64, i64, i64)>>,
}

// Borders
fn style_factor(style: &str) -> f64 {
    match style {
        "double" => 3.0,
        "triple" => 3.5,
        "dashed" | "dotted" | "dashSmallGap" | "dotDash" | "dotDotDash" => 0.75,
        _ => 1.0,
    }
}

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

/// A border a style draws: not an art border.
pub(super) fn drawn(b: &Border) -> bool {
    art(b).is_none()
}

fn art(b: &Border) -> Option<f64> {
    (!ORDER.contains(&b.style.as_str())).then(|| (b.width * 8.0).min(255.0))
}

fn conflict(a: (Option<&Border>, bool), b: (Option<&Border>, bool)) -> Option<Border> {
    winner(a, b).cloned()
}

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

pub(super) fn thick(b: &Border) -> f64 {
    stripes(b).iter().sum()
}

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

fn box_h(b: &Option<Border>) -> f64 {
    match b.as_ref().filter(|b| b.visible() && art(b).is_some()) {
        Some(b) => b.space.clamp(0.0, 31.0) + if b.shadow { 2.0 } else { 1.0 } * ((b.width * 20.0 + 1e-6).floor() / 20.0).max(0.05),
        None => space_h(b),
    }
}

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

pub(super) fn edge(out: &mut Vec<Item>, rect: [f64; 4], horizontal: bool, far: bool, b: &Border) {
    let colour = b.color.unwrap_or(0);
    let [x0, y0, x1, y1] = rect;
    let st = stripes(b);
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
    match b.style.as_str() {
        "wave" => return wave(out, rect, horizontal, 0.0, colour),
        "doubleWave" => {
            let gap = if horizontal { y1 - y0 } else { x1 - x0 }.max(0.25) * 2.0;
            wave(out, rect, horizontal, -gap / 2.0, colour);
            return wave(out, rect, horizontal, gap / 2.0, colour);
        }
        // Outset is light at the top and left and dark at the bottom and right; inset the other way.
        "outset" | "inset" => {
            let [light, _, dark] = tones(b.color.unwrap_or(0xA0A0A0));
            let lit = (b.style == "outset") != far;
            out.push(Item::rect(rect, if lit { light } else { dark }));
            return;
        }
        _ => {}
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

/// A wave along the edge, `shift` from its middle: steps of a sine as thin as the line, two line widths high.
fn wave(out: &mut Vec<Item>, rect: [f64; 4], horizontal: bool, shift: f64, colour: u32) {
    let [x0, y0, x1, y1] = rect;
    let t = if horizontal { y1 - y0 } else { x1 - x0 }.max(0.25);
    let mid = if horizontal { (y0 + y1) / 2.0 } else { (x0 + x1) / 2.0 } + shift;
    let (start, end) = if horizontal { (x0, x1) } else { (y0, y1) };
    let (amp, period, step) = (t, 6.0 * t, (t / 2.0).max(0.25));
    let mut at = start;
    while at < end - EPS {
        let to = (at + step).min(end);
        let c = mid + amp * (std::f64::consts::TAU * (at - start) / period).sin();
        out.push(Item::rect(if horizontal { [at, c - t / 2.0, to, c + t / 2.0] } else { [c - t / 2.0, at, c + t / 2.0, to] }, colour));
        at = to;
    }
}

fn three_d(b: &Border) -> Option<[u32; 3]> {
    let engrave = match b.style.as_str() {
        "threeDEmboss" => false,
        "threeDEngrave" => true,
        _ => return None,
    };
    let [light, middle, dark] = tones(b.color.unwrap_or(0xA0A0A0));
    Some(if engrave { [dark, middle, light] } else { [light, middle, dark] })
}

/// A 3-D border's light, middle and dark tones of its color.
fn tones(c: u32) -> [u32; 3] {
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
    [light, middle, dark]
}

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

fn half_border(w: f64) -> f64 {
    let tw = (w * 20.0).round() as i64;
    ((tw + 1) / 2) as f64 / 20.0
}

pub fn outer_box(tb: &TableLayout) -> (f64, f64) {
    let ends = |r: &RowBox| {
        let last = r.cells.len().checked_sub(1)?;
        Some(if tb.rtl { (last, 0) } else { (0, last) })
    };
    tb.rows.iter().filter_map(|r| ends(r).map(|(a, z)| (&r.cells[a], &r.cells[z]))).fold((f64::INFINITY, f64::NEG_INFINITY), |(l, r), (a, z)| {
        let (al, zr) = (drawn_edges(tb, a)[1], drawn_edges(tb, z)[3]);
        (l.min(tb.x0 + a.x - half_border(bw(al))), r.max(tb.x0 + z.x + z.w + bw(zr) / 2.0))
    })
}

pub fn nested_shift(t: &Table) -> f64 {
    if rtl(t) {
        let edges = cell_edges(t, grid_columns(t), &|_| None);
        let res = rtl_sides(edges.first().into_iter().flatten().map(|c| &*c.edges));
        return rtl_x0(t, 0.0, 0.0, res, false) - rtl_x0(t, 0.0, 0.0, res, true);
    }
    if matches!(t.jc, Jc::Center | Jc::Right) {
        return 0.0;
    }
    let (lw, lmar) = t
        .rows
        .first()
        .and_then(|r| r.cells.first())
        .map_or((space_h(&t.borders[1]), 0.0), |c| (space_h(candidate(Some(&c.left), &t.borders[1])), c.margins[1]));
    lw / 2.0 + edge_gap(t, lw, lmar)
}

fn rtl(t: &Table) -> bool {
    t.bidi_visual && t.floating.is_none()
}

fn rtl_sides<'a>(mut row: impl Iterator<Item = &'a [Option<Border>; 4]>) -> (i64, i64) {
    let first = row.next();
    let last = row.last().or(first);
    (first.map_or(0, |e| twips(space_h(&e[3]))), last.map_or(0, |e| twips(space_h(&e[1]))))
}

fn rtl_x0(t: &Table, total: f64, text_w: f64, res: (i64, i64), sloppy: bool) -> f64 {
    let row = t.rows.first();
    let (first, last) = (row.and_then(|r| r.cells.first()), row.and_then(|r| r.cells.last()));
    let side = |own: Option<&Border>, table: &Option<Border>| twips(space_h(&conflict((own, true), (table.as_ref(), false))));
    let b_start = side(first.and_then(|c| c.left.as_ref()), &t.borders[1]);
    let b_end = side(last.and_then(|c| c.right.as_ref()), &t.borders[3]);
    let alone = row.is_some_and(|r| r.grid_before == 0 && r.cells.len() == 1);
    let first_outer = row.is_some_and(|r| r.grid_after == 0 && r.cells.len() == 1);
    let b_last = side(last.and_then(|c| c.left.as_ref()), &t.borders[if alone { 1 } else { 5 }]);
    let hb = |b: i64| (b + 1) / 2;
    let (w, width, ind, s) = (twips(text_w), twips(total), twips(t.ind), twips(t.cell_spacing.max(0.0)));
    let margin = |c: Option<&Cell>| c.map_or(0, |c| twips(c.margins[1]));
    let gap = |outer: bool, b: i64| if s > 0 { s * if outer { 2 } else { 1 } + b } else { 0 };
    let extra = (b_start + b_last) / 2;
    let left = match t.jc {
        Jc::Right if sloppy => -hb(b_end).max(margin(last) + gap(alone, res.1)),
        Jc::Right => 0,
        Jc::Center if sloppy => (w - width).div_euclid(2),
        Jc::Center => (w - width - extra).div_euclid(2),
        _ if sloppy => w - ind + hb(b_start).max(margin(first) + gap(first_outer, res.0)) - width,
        _ => w - ind - extra - width,
    };
    left as f64 / 20.0
}

fn candidate<'b>(own: Option<&'b Option<Border>>, table: &'b Option<Border>) -> &'b Option<Border> {
    match own {
        Some(o @ Some(_)) => o,
        _ => table,
    }
}

fn edge_add(compat: Compat, t: &Table) -> (f64, f64) {
    let first = t.rows.first();
    let (mut lc, mut rc) = (first.and_then(|r| r.cells.first()), first.and_then(|r| r.cells.last()));
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

fn row_borders(t: &Table, ri: usize) -> &[Option<Border>; 6] {
    t.rows.get(ri).and_then(|r| r.borders.as_deref()).unwrap_or(&t.borders)
}

fn edge_right(t: &Table) -> f64 {
    let last = t.rows.first().and_then(|r| if rtl(t) { r.cells.first() } else { r.cells.last() });
    space_h(candidate(last.map(|c| &c.right), &row_borders(t, 0)[3]))
}

fn edge_gap(t: &Table, b: f64, mar: f64) -> f64 {
    half_border(b).max(spacing_of(t, 0).map_or(mar, |s| 2.0 * s + b + mar))
}

/// The row's cell spacing, `None` when it has none. A row whose own spacing is 0 still frames its cells.
fn spacing_of(t: &Table, ri: usize) -> Option<f64> {
    t.rows.get(ri).and_then(|r| r.cell_spacing).or((t.cell_spacing > 0.0).then_some(t.cell_spacing))
}

fn framed(t: &Table, ri: usize) -> bool {
    spacing_of(t, ri).is_some()
}

fn gap(s: f64, outer: bool) -> f64 {
    if outer { 2.0 * s } else { s }
}

// Content widths
fn content_widths(blocks: &[Block], env: &StoryEnv, fixed: bool, memos: &mut Memos) -> Result<Rc<(i64, i64, i64)>, Yield> {
    let laid = super::join::laid(blocks, env.ctx.env.text.review.view);
    let cell = Some(CellEnds { fixed, ..CellEnds::default() });
    let probe = StoryKey::new(child_ids(&laid, None, env, cell, memos)?, None, cell, env, &laid);
    cached(memos, |c| &mut c.cell_widths, probe, |_| std::mem::size_of::<(i64, i64, i64)>(), |m| content_widths_uncached(&laid, env, fixed, m))
}

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
            w += (h * 1440 + 294_911) / 294_912;
        }
    }
    Ok(w)
}

fn up(x: f64) -> i64 {
    (x * 20.0 - 1e-6).ceil().max(0.0) as i64
}

fn twips(x: f64) -> i64 {
    (x * 20.0).round() as i64
}

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
                let text_of = |q: &super::text::Piece| shaped.text.get(q.text.0 as usize..q.text.1 as usize).unwrap_or("");
                let mut tail = vec![false; pieces.len() + 1];
                tail[pieces.len()] = true;
                for i in (0..pieces.len()).rev() {
                    let t = text_of(&pieces[i]);
                    tail[i] = tail[i + 1] && (pieces[i].kind == Kind::Space || (!t.is_empty() && t.chars().all(|c| c == '-')));
                }
                let is_num = |q: &super::text::Piece| q.src.is_none() && matches!(q.kind, Kind::ListNumber | Kind::Tab | Kind::Space);
                let numbered = old && pieces.iter().any(|q| q.kind == Kind::ListNumber);
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
                        word = ends[i] - s.ind_left;
                        continue;
                    }
                    match pc.kind {
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
                for (li, l) in lines.iter().enumerate() {
                    let start_x = if li == 0 { start0 } else { s.ind_left };
                    let shift = l.xs.first().map_or(0.0, |&x| (x - start_x).max(0.0));
                    let end = (0..l.xs.len())
                        .rev()
                        .find(|&k| !matches!(pieces.get(l.start + k).map(|q| q.kind), Some(Kind::Space)))
                        .map_or(start_x, |k| l.xs[k] + l.widths[k] - shift);
                    let from = if numbered && li == 0 { 0.0 } else { start_x };
                    let mut w = if old { (end - from).max(0.0) + ind } else { end.max(0.0) + s.ind_right };
                    if !old && li + 1 == lines.len() && start_x < 0.0 {
                        let full = l.xs.last().zip(l.widths.last()).map_or(start_x, |(x, w)| x + w - shift);
                        let mark = p.mark_in(env.ctx.env.text.review.view);
                        let glyph = if bi + 1 == blocks.len() { '\u{a4}' } else { '\u{b6}' };
                        let mark_w = env
                            .ctx
                            .fonts
                            .face(&mark.ascii, None, mark.bold, mark.italic)
                            .and_then(|f| Some(f.advance(f.glyph(glyph)?) * f.scale(mark.size)));
                        w = w.max(full + mark_w.unwrap_or(0.0) + s.ind_right);
                    }
                    mx = mx.max(up(w));
                }
            }
            &LaidBlock::Table(t) => {
                let t = super::story::as_nested(t, Some(CellEnds { fixed, ..CellEnds::default() }));
                let shown = in_view(&t, env.ctx.env.text.review.view, env.ctx.expansions);
                let t = shown.as_ref().map_or(&*t, |v| &v.table);
                let f = fit(t, 0.0, env, Some(autofit::MAXW), memos)?;
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

// Column widths
struct Fit {
    cols: Vec<f64>,
    min_t: i64,
    width: i64,
    hard_t: i64,
}

fn fit(t: &Table, text_w: f64, env: &StoryEnv, avail: Option<i64>, memos: &mut Memos) -> Result<Fit, Yield> {
    let edges = cell_edges(t, grid_columns(t), &|_| None);
    let inputs = fit_inputs(t, env, memos, &edges, &|_| None)?;
    Ok(resolve_fit(t, text_w, env, avail, &inputs))
}

fn grid_columns(t: &Table) -> usize {
    t.grid.len().max(t.rows.iter().map(|r| r.grid_before + r.cells.iter().map(|c| c.span).sum::<usize>() + r.grid_after).max().unwrap_or(0))
}

fn fit_inputs<'k>(
    t: &Table,
    env: &StoryEnv,
    memos: &mut Memos,
    geo: &[Vec<CellEdges>],
    kept: &dyn Fn(usize) -> Option<&'k FitRow>,
) -> Result<Vec<FitRow>, Yield> {
    let n = grid_columns(t);
    let grid_tw: Vec<i64> = (0..n).map(|i| twips(t.grid.get(i).copied().unwrap_or(0.0))).collect();
    let mut rows = Vec::with_capacity(t.rows.len());
    for (ri, (r, gs)) in t.rows.iter().zip(geo).enumerate() {
        let row_s = spacing_of(t, ri);
        let s2 = twips(2.0 * row_s.unwrap_or(0.0));
        let spacing = twips(row_s.unwrap_or(0.0));
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
            let gap = |b: i64, m: i64| if row_s.is_some() { spacing + b + m } else { m };
            let pad = (gap(bl, ml) + gap(br, mr)).max((bl + br + 1) / 2);
            let cell_pad = (pad - 2 * spacing).max(0);
            let continuation = c.v_merge == VMerge::Continue;
            let grid_w: i64 = grid_tw[g.col..g.col + g.span].iter().sum();
            let from_grid = grid_w - s2 - s2 / 2 * (i64::from(g.col == 0) + i64::from(g.col + g.span >= n));
            let (kind, pref) = match c.width {
                Some(Width::Dxa(w)) => (autofit::Kind::Dxa, twips(w)),
                Some(Width::Pct(f)) => (autofit::Kind::Pct, (f * 5000.0).round() as i64),
                Some(Width::Nil) => (autofit::Kind::Dxa, from_grid),
                Some(Width::Auto) | None if t.fixed => (autofit::Kind::Dxa, from_grid),
                Some(Width::Auto) | None => (autofit::Kind::Auto, 0),
            };
            let (min, max, hard) = if t.fixed {
                (cell_pad + 20, cell_pad + 20, cell_pad)
            } else {
                let rotated = matches!(c.text_direction.as_str(), "btLr" | "tbRl" | "tbRlV" | "tbLrV");
                let (mut cmn, mut cmx, chard) = if continuation {
                    (0, 0, 0)
                } else if let (true, Some(Width::Dxa(w))) = (c.fit_text, &c.width) {
                    let w = (twips(*w) - cell_pad).max(0);
                    (w, w, w)
                } else if rotated {
                    let w = vertical_width(&c.blocks, env, memos)?;
                    (w, w, 0)
                } else {
                    let w = content_widths(&c.blocks, env, t.fixed, memos)?;
                    widths.push(w.clone());
                    *w
                };
                if (cmn, cmx) == (0, 0) && !continuation {
                    (cmn, cmx) = (6, 6);
                }
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

fn resolve_fit(t: &Table, text_w: f64, env: &StoryEnv, avail: Option<i64>, inputs: &[FitRow]) -> Fit {
    let n = grid_columns(t);
    let (add_l, add_r) = edge_add(env.ctx.env.text.compat, t);
    let a = avail.unwrap_or_else(|| twips(text_w - t.ind + add_l + add_r));
    let s2 = twips(2.0 * t.cell_spacing.max(0.0));
    let cells: Vec<autofit::Cell> = inputs.iter().flat_map(|r| r.cells.iter().copied()).collect();
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
    Fit { cols: widths.iter().map(|&w| w as f64 / 20.0).collect(), min_t: r.min_t, width: r.width, hard_t: r.hard_t }
}

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

pub fn layout(t: &Table, text_w: f64, env: &StoryEnv, prev: Option<&ParaStyle>, measure: bool, memos: &mut Memos) -> Result<Rc<TableLayout>, Yield> {
    layout_with(t, text_w, env, prev, measure, None, memos)
}

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
    if let Some(runs) = layout_runs(t) {
        return layout_runs_of(t, &runs, text_w, env, prev, measure, memos);
    }
    match in_view(t, env.ctx.env.text.review.view, env.ctx.expansions) {
        None => layout_shown(t, text_w, env, prev, measure, reuse, memos),
        Some(v) => {
            let mut tb = v.expand(&*layout_shown(&v.table, text_w, env, prev, measure, None, memos)?);
            if hidden_tail(t)
                && env.ctx.env.text.compat == Compat::Legacy
                && let Some(tb) = Rc::get_mut(&mut tb)
            {
                tb.bottom_border = 0.0;
            }
            Ok(tb)
        }
    }
}

// Layout runs
fn layout_runs(t: &Table) -> Option<Vec<std::ops::Range<usize>>> {
    if t.fixed || t.rows.iter().all(|r| r.run.is_none()) {
        return None;
    }
    let mut runs: Vec<std::ops::Range<usize>> = Vec::new();
    for ri in 0..t.rows.len() {
        match runs.last_mut() {
            Some(run) if t.rows[run.start].run.is_some() == t.rows[ri].run.is_some() => run.end = ri + 1,
            _ => runs.push(ri..ri + 1),
        }
    }
    (runs.len() > 1).then_some(runs)
}

fn layout_runs_of(
    t: &Table,
    runs: &[std::ops::Range<usize>],
    text_w: f64,
    env: &StoryEnv,
    prev: Option<&ParaStyle>,
    measure: bool,
    memos: &mut Memos,
) -> Result<Rc<TableLayout>, Yield> {
    let mut merged: Option<TableLayout> = None;
    let mut fit: Vec<FitRow> = Vec::new();
    let n = runs.len();
    // ponytail: assumes no tblPr jc when the first row has its own (compat 15 reads the table's from it); keep the tblPr jc on Table if that shows up.
    let table_jc = if env.ctx.env.text.compat == Compat::Generic && t.rows[0].jc.is_some() { Jc::Left } else { t.jc };
    for (k, run) in runs.iter().enumerate() {
        let first = &t.rows[run.start];
        let mut sub = Table { rows: t.rows[run.clone()].to_vec(), ..t.clone() };
        if k > 0 {
            sub.jc = first.jc.unwrap_or(table_jc);
        }
        if first.run.is_some() {
            sub.borders = row_borders(t, run.start).clone();
            sub.rows.iter_mut().for_each(|r| r.run = None);
        }
        // The first run keeps the table's own layout.
        if let Some(width) = first.run.filter(|_| k > 0) {
            sub.fixed = true;
            sub.width = width;
            if let Some(ind) = first.ind {
                sub.ind = ind;
                sub.ind_set = true;
            }
        }
        let laid = layout_with(&sub, text_w, env, if k == 0 { prev } else { None }, measure, None, memos)?;
        fit.extend(laid.fit.iter().cloned());
        let shift = merged.as_ref().map_or(0.0, |m| laid.x0 - m.x0);
        let mut rows: Vec<RowBox> = laid.rows.clone();
        for r in &mut rows {
            r.cells.iter_mut().for_each(|c| c.x += shift);
            r.seams.iter_mut().for_each(|s| {
                s.0 += shift;
                s.1 += shift;
            });
        }
        if k + 1 < n
            && let Some(last) = rows.last_mut()
        {
            last.height += laid.bottom_border;
            last.trail += laid.bottom_border;
        }
        match merged.as_mut() {
            None => merged = Some(TableLayout { rows, fit: Rc::from([]), ..clone_layout(&laid) }),
            Some(m) => {
                m.rows.extend(rows);
                m.bottom_border = laid.bottom_border;
            }
        }
    }
    let mut m = merged.expect("a run");
    m.fit = fit.into();
    Ok(Rc::new(m))
}

fn clone_layout(tb: &TableLayout) -> TableLayout {
    TableLayout {
        x0: tb.x0,
        cols: tb.cols.clone(),
        rows: Vec::new(),
        bottom_border: tb.bottom_border,
        spacing: tb.spacing,
        id: tb.id,
        outer: tb.outer.clone(),
        fit: tb.fit.clone(),
        rtl: tb.rtl,
    }
}

// Markup view
struct ViewTable {
    table: Table,
    rows: Vec<Option<Vec<usize>>>,
}

fn in_view(t: &Table, view: MarkupView, expansions: Option<&super::expansion::Expansions>) -> Option<ViewTable> {
    let original = view == MarkupView::Original;
    let grid = t.table_grid_in(view);
    let differs = |c: &Cell| view.hides_cell(&c.rev) || c.v_merge_in(view) != c.v_merge || original && c.rev.change.is_some();
    let hides = t.rows.iter().any(|r| r.vanished || view.hides_row(&r.rev) || original && r.rev.change.is_some() || r.cells.iter().any(differs));
    let shown_row = |ri: usize| expansions.and_then(|e| e.rows.get(&(super::ids::TableId(t.id), ri)));
    let expanded = expansions.is_some_and(|e| !e.rows.is_empty()) && (0..t.rows.len()).any(|ri| shown_row(ri).is_some());
    if !hides && !expanded && grid == t.grid.as_slice() && !(original && t.change.is_some()) {
        return None;
    }
    let mut table = t.clone();
    table.grid = grid.to_vec();
    carry_hidden_bottoms(&mut table);
    if let Some(p) = t.change.as_ref().and_then(|c| c.previous.as_ref()).filter(|_| original) {
        p.restore_table(&mut table);
    }
    let previous = |c: &Option<std::rc::Rc<crate::docx::review::TableChange>>| c.as_ref().filter(|_| original).and_then(|c| c.previous.clone());
    let mut rows = Vec::with_capacity(t.rows.len());
    table.rows = std::mem::take(&mut table.rows)
        .into_iter()
        .enumerate()
        .filter_map(|(ri, mut r)| {
            if let Some(body) = shown_row(ri) {
                let n = r.cells.len().max(1);
                r.cells = body.cells.clone();
                rows.push(Some((0..r.cells.len()).map(|k| k.min(n - 1)).collect()));
                return Some(r);
            }
            if r.vanished || view.hides_row(&r.rev) {
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
                    lead: 0.0,
                    trail: 0.0,
                    div_boxes: Rc::from([]),
                    shading: None,
                    spacing: None,
                    closes: false,
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
    let near = |ri: usize| reuse.as_ref().is_some_and(|(_, rows)| ri + 1 >= *rows.start() && ri <= rows.end() + 1);
    let edges = crate::prof::time("table:edges", || cell_edges(t, n, &|ri| reuse.as_ref().filter(|_| !near(ri)).map(|(o, _)| &o.rows[ri])));
    let inputs = crate::prof::time("table:fit", || fit_inputs(t, env, memos, &edges, &|ri| kept(ri).map(|o| &o.fit[ri])))?;
    let Fit { cols, min_t, width, hard_t } = resolve_fit(t, text_w, env, None, &inputs);
    let geo = if cols.len() == n { place(t, &cols, &edges) } else { cell_geometry(t, &cols) };

    let mut stories = Vec::new();
    let mut closings = Vec::new();
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

// Hidden rows
fn carry_hidden_bottoms(t: &mut Table) {
    let starts = |r: &Row| {
        let mut col = r.grid_before;
        r.cells
            .iter()
            .map(|c| {
                let s = col;
                col += c.span;
                s
            })
            .collect::<Vec<usize>>()
    };
    for ri in (1..t.rows.len()).rev() {
        if !t.rows[ri].vanished || t.rows[ri - 1].vanished {
            continue;
        }
        let below = ri + t.rows[ri..].iter().take_while(|r| r.vanished).count() - 1;
        let hidden = &t.rows[below];
        let at = starts(hidden);
        let bottoms: Vec<(usize, usize, Option<Border>)> = hidden.cells.iter().zip(&at).map(|(c, &s)| (s, s + c.span, c.bottom.clone())).collect();
        let inside = hidden.borders.as_ref().map(|b| b[4].clone());
        let above = &mut t.rows[ri - 1];
        let at = starts(above);
        for (c, s) in above.cells.iter_mut().zip(at) {
            c.bottom = bottoms.iter().find(|(a, b, _)| s >= *a && s < *b).and_then(|x| x.2.clone());
        }
        if let Some(inside) = inside {
            let mut b = above.borders.as_deref().cloned().unwrap_or_else(|| t.borders.clone());
            b[4] = inside;
            above.borders = Some(Rc::new(b));
        }
    }
}

fn hidden_tail(t: &Table) -> bool {
    t.rows.last().is_some_and(|r| r.vanished)
}

fn mark_only(p: &crate::docx::Paragraph) -> bool {
    p.inlines.iter().all(|i| match i {
        Inline::Text(t, r) => t.is_empty() || r.hidden,
        Inline::Bookmark(..) | Inline::Range(..) => true,
        i => i.run().is_some_and(|r| r.hidden),
    })
}

fn closing_para(c: &Cell) -> Option<(ParaId, f64)> {
    let bordered = |s: &crate::docx::ParaStyle| {
        [&s.border_top, &s.border_left, &s.border_bottom, &s.border_right, &s.border_between].iter().any(|b| b.is_some())
    };
    match c.blocks.as_slice() {
        [.., Block::Table(_), Block::Para(p)] if p.inlines.is_empty() => Some((ParaId(p.id), p.mark.size)),
        [.., Block::Para(p)] if c.hide_mark && mark_only(p) && p.list.is_none() && !bordered(&p.style) => Some((ParaId(p.id), p.mark.size)),
        _ => None,
    }
}

fn holds_float(c: &Cell) -> bool {
    c.blocks.iter().any(|b| matches!(b, Block::Para(p) if p.inlines.iter().any(|i| matches!(i, Inline::Float(f, _) if f.in_cell(Compat::Legacy)))))
}

// Cell geometry
fn cell_geometry(t: &Table, cols: &[f64]) -> Vec<Vec<CellGeo>> {
    place(t, cols, &cell_edges(t, cols.len(), &|_| None))
}

struct CellEdges {
    ci: usize,
    col: usize,
    span: usize,
    edges: Rc<[Option<Border>; 4]>,
}

fn cell_edges<'k>(t: &Table, ncols: usize, kept: &dyn Fn(usize) -> Option<&'k RowBox>) -> Vec<Vec<CellEdges>> {
    let nrows = t.rows.len();
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
    let mut grid_cells: Vec<Vec<(usize, usize)>> = Vec::new();
    for r in &t.rows {
        let mut col = r.grid_before;
        let mut v = Vec::new();
        for c in &r.cells {
            v.push((col, c.span));
            col += c.span;
        }
        grid_cells.push(v);
    }
    let mut out = Vec::new();
    for (ri, r) in t.rows.iter().enumerate() {
        let alone = framed(t, ri);
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
            let tb = row_borders(t, ri);
            let top_t = if ri == 0 { &tb[0] } else { &tb[4] };
            let bot_t = if ri + 1 == nrows { &tb[2] } else { &tb[4] };
            let left_t = if col == 0 { &tb[1] } else { &tb[5] };
            let right_t = if col + span >= ncols { &tb[3] } else { &tb[5] };
            let (top, left, bottom, right) = (side(&c.top, top_t), side(&c.left, left_t), side(&c.bottom, bot_t), side(&c.right, right_t));
            let neighbour = |rr: usize, cc: usize| -> Option<&Cell> {
                t.rows.get(rr).and_then(|row| grid_cells[rr].iter().position(|&(c0, s)| cc >= c0 && cc < c0 + s).map(|i| &row.cells[i]))
            };
            let top_e = if ri == 0 || alone || framed(t, ri - 1) {
                conflict((top.b, top.cell), (None, false))
            } else {
                let above = neighbour(ri - 1, col);
                let ab = above.map(|a| side(&a.bottom, &row_borders(t, ri - 1)[4]));
                conflict((top.b, top.cell), ab.map_or((None, false), |s| (s.b, s.cell)))
            };
            let bot_e = if ri + 1 == nrows || alone || framed(t, ri + 1) {
                conflict((bottom.b, bottom.cell), (None, false))
            } else {
                let below = neighbour(ri + 1, col);
                let bl = below.map(|a| side(&a.top, &row_borders(t, ri + 1)[4]));
                conflict((bottom.b, bottom.cell), bl.map_or((None, false), |s| (s.b, s.cell)))
            };
            let left_e = if ci == 0 || alone {
                conflict((left.b, left.cell), (None, false))
            } else {
                let lc = &r.cells[ci - 1];
                let ls = side(&lc.right, &tb[5]);
                conflict((left.b, left.cell), (ls.b, ls.cell))
            };
            let right_e = if ci + 1 == r.cells.len() || alone {
                conflict((right.b, right.cell), (None, false))
            } else {
                let rc = &r.cells[ci + 1];
                let rs = side(&rc.left, &tb[5]);
                conflict((right.b, right.cell), (rs.b, rs.cell))
            };
            cells.push(CellEdges { ci, col, span, edges: Rc::new([top_e, left_e, bot_e, right_e]) });
        }
        out.push(cells);
    }
    out
}

fn place(t: &Table, cols: &[f64], edges: &[Vec<CellEdges>]) -> Vec<Vec<CellGeo>> {
    let col_x: Vec<i64> = std::iter::once(0)
        .chain(cols.iter().scan(0, |acc, w| {
            *acc += twips(*w);
            Some(*acc)
        }))
        .collect();
    let pt = |v: i64| v as f64 / 20.0;
    let ncols = cols.len();
    edges
        .iter()
        .zip(&t.rows)
        .enumerate()
        .map(|(ri, (row, r))| {
            row.iter()
                .map(|e| {
                    let (c, col, span) = (&r.cells[e.ci], e.col, e.span);
                    let (mut left_e, mut right_e) = (&e.edges[1], &e.edges[3]);
                    let (mut left_outer, mut right_outer) = (col == 0, col + span >= ncols);
                    if rtl(t) {
                        (left_e, right_e, left_outer, right_outer) = (right_e, left_e, right_outer, left_outer);
                    }
                    let (x, w) = (col_x[col], col_x[col + span] - col_x[col]);
                    let (ml, mr) = (twips(c.margins[1]), twips(c.margins[3]));
                    let (cx, cw) = if let Some(row_s) = spacing_of(t, ri) {
                        let (gl, gr) = (twips(gap(row_s, left_outer)), twips(gap(row_s, right_outer)));
                        let (bl, br) = (twips(space_h(left_e)), twips(space_h(right_e)));
                        // Without a gap the text starts half the border further left; the width stays.
                        let indent = if gl > 0 { ml } else { (ml - bl / 2).max(0) };
                        (gl + bl + indent, (w - gl - bl - ml - mr - br - gr).max(0))
                    } else {
                        let (bl, br) = (twips(box_h(left_e)), twips(box_h(right_e)));
                        (ml.max(bl / 2), (w - (ml + mr).max((bl + br) / 2)).max(0))
                    };
                    CellGeo { ci: e.ci, col, span, x: pt(x), w: pt(w), cx: pt(cx), cw: pt(cw), edges: e.edges.clone() }
                })
                .collect()
        })
        .collect()
}

// Row assembly
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
    let first_left = t.rows.first().and_then(|r| r.cells.first()).map(|c| (space_h(candidate(Some(&c.left), &row_borders(t, 0)[1])), c.margins[1]));
    let (lw, lmar) = first_left.unwrap_or((space_h(&row_borders(t, 0)[1]), 0.0));
    let mut x0 = match compat {
        Compat::Generic => t.ind + half_border(lw),
        Compat::Legacy if !t.ind_set => t.ind,
        Compat::Legacy => t.ind - edge_gap(t, lw, lmar),
    };
    match t.jc {
        Jc::Center => {
            let (gb, ga) = t.rows.first().map_or((0, 0), |r| (r.grid_before.min(cols.len()), r.grid_after.min(cols.len())));
            let before: f64 = cols[..gb].iter().sum();
            let after: f64 = cols[cols.len() - ga.min(cols.len() - gb)..].iter().sum();
            let (lh, rh) = (twips(half_border(lw)), twips(edge_right(t)) / 2);
            let free = twips(text_w) - (lh + twips(total - before - after) + rh);
            x0 = (lh + (free + 1).div_euclid(2)) as f64 / 20.0 - before;
        }
        Jc::Right => x0 = (twips(text_w) - twips(total) - twips(edge_right(t)) / 2) as f64 / 20.0,
        _ => {}
    }
    // Floating tables
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
    let rtl = rtl(t);
    if rtl {
        x0 = rtl_x0(t, total, text_w, rtl_sides(geo.first().into_iter().flatten().map(|g| &*g.edges)), compat == Compat::Legacy);
    }
    let mirror = |x: f64, w: f64| if rtl { total - x - w } else { x };
    let mut stories = stories.iter();
    let mut closings = closings.iter();
    let mut rows: Vec<RowBox> = Vec::new();
    let review = &env.ctx.env.text.review;
    for ((ri, r), cells_geo) in t.rows.iter().enumerate().zip(geo) {
        let row_s = spacing_of(t, ri);
        let shift = match r.ind {
            Some(i) if !t.fixed && t.jc == Jc::Left && t.floating.is_none() && !rtl => i - t.ind,
            _ => 0.0,
        } + r.div.as_ref().filter(|_| t.jc == Jc::Left && t.floating.is_none() && !rtl).map_or(0.0, |d| d.shift);
        // Row alignment
        let shift = shift
            + match r.jc.unwrap_or(t.jc) {
                jc @ (Jc::Center | Jc::Right) if compat == Compat::Legacy && t.floating.is_none() && !rtl => {
                    let n = cols.len();
                    let (gb, ga) = if env.ctx.env.text.align_tables_row_by_row { (r.grid_before.min(n), r.grid_after) } else { (0, 0) };
                    let before: f64 = cols[..gb].iter().sum();
                    let after: f64 = cols[n - ga.min(n - gb)..].iter().sum();
                    let width = total - before - after;
                    let gap = r.cells.first().map_or(0.0, |c| c.margins[1]) + row_s.map_or(0.0, |s| 2.0 * s + space_h(&t.borders[1]));
                    let left = if jc == Jc::Center { (twips(text_w) - twips(width)).div_euclid(2) as f64 / 20.0 } else { text_w - width + gap };
                    left - (x0 + before)
                }
                Jc::Left if compat == Compat::Legacy && t.floating.is_none() && !rtl && t.jc != Jc::Left => -edge_gap(t, lw, lmar) - x0,
                _ => 0.0,
            };
        let mut cells = Vec::new();
        for g in cells_geo {
            let c = &r.cells[g.ci];
            let shading = tint(review, r, c);
            let units = stories.next().cloned().flatten().unwrap_or_default();
            let content_h = units.height;
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
        let changed = review.view.change_bars()
            && (r.rev.ins.is_some()
                || r.rev.del.is_some()
                || r.rev.change.is_some()
                || cells_geo.iter().any(|g| r.cells[g.ci].rev.structural.is_some() || r.cells[g.ci].rev.change.is_some()));
        let mut top_border = cells.iter().map(|c| space_h(&c.edges[0])).fold(0.0, f64::max);
        let seams: Vec<(f64, f64, Border)> = match ri.checked_sub(1).and_then(|p| geo.get(p)) {
            Some(above) if row_s.is_none() && !framed(t, ri - 1) => above
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
        if row_s.is_none() {
            let cand = |own: &Option<Border>, table: &Option<Border>| space_h(if own.is_some() { own } else { table });
            let top_t = if ri == 0 { &row_borders(t, ri)[0] } else { &row_borders(t, ri)[4] };
            let w = ri
                .checked_sub(1)
                .filter(|&p| !framed(t, p))
                .map_or(0.0, |p| t.rows[p].cells.iter().map(|c| cand(&c.bottom, &row_borders(t, p)[4])).fold(0.0, f64::max));
            let above = cells_geo.iter().map(|g| cand(&r.cells[g.ci].top, top_t) + r.cells[g.ci].margins[0]).fold(w, f64::max);
            for c in &mut cells {
                c.margins[0] = (above - top_border).max(0.0);
            }
        }
        let mut gap_top = 0.0;
        let mut cell_bottom = 0.0;
        if let Some(s) = row_s {
            gap_top = if ri == 0 { 2.0 * s + space_h(&t.borders[0]) } else { s };
            // An unframed row above leaves its bottom border to this row's top.
            if let Some(p) = ri.checked_sub(1).filter(|&p| !framed(t, p)) {
                let w = rows[p].cells.iter().map(|c: &CellBox| space_h(&c.edges[2])).fold(0.0, f64::max);
                let own = top_border + s + cells.iter().map(|c| c.margins[0]).fold(0.0, f64::max);
                gap_top += (w - own).max(0.0);
            }
            top_border += gap_top;
            cell_bottom = cells.iter().map(|c| space_h(&c.edges[2])).fold(0.0, f64::max) + gap(s, ri + 1 == t.rows.len());
        }
        let below = cells.iter().map(|c| c.margins[2]).fold(0.0, f64::max);
        let content = cells.iter().filter(|c| c.v_merge == VMerge::None).map(|c| c.margins[0] + c.content_h).fold(f64::NEG_INFINITY, f64::max);
        let content = if content.is_finite() { content + below + cell_bottom } else { 0.0 };
        let min_margins = cells.iter().map(|c| c.margins[0] + c.margins[2]).fold(0.0, f64::max);
        let content = content.max(min_margins);
        let bottom = cells.iter().map(|c| c.margins[2]).fold(0.0, f64::max);
        let (height, exact, min_h) = match &r.height {
            Some((h, rule)) if rule == "exact" && *h > 0.0 => (*h + bottom, true, 0.0),
            Some((h, rule)) if rule != "auto" => (top_border + content.max(*h + min_margins), false, top_border + *h + min_margins),
            _ => (top_border + content, false, 0.0),
        };
        let first = match r.cells.first().and_then(|c| c.blocks.first()) {
            Some(Block::Para(p)) => Some(p.para_style_in(env.ctx.env.text.review.view)),
            _ => None,
        };
        let closes = row_s.is_none() && ri + 1 < t.rows.len() && framed(t, ri + 1);
        let (lead, trail) = r.div.as_ref().map_or((0.0, 0.0), |d| (d.lead, d.trail));
        rows.push(RowBox {
            cells,
            top_border,
            height: height + lead + trail,
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
            lead,
            trail,
            div_boxes: r.div.as_ref().map_or_else(|| Rc::from([]), |d| d.boxes.clone()),
            shading: r.shading.filter(|_| row_s.is_some_and(|s| s > 0.0)),
            spacing: row_s,
            closes,
        });
    }
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
    let mut bottom_border = rows.last().map(|r| r.cells.iter().map(|c| space_h(&c.edges[2])).fold(0.0, f64::max)).unwrap_or(0.0);
    if framed(t, t.rows.len().saturating_sub(1)) {
        bottom_border = space_h(&t.borders[2]);
    }
    let (l, r) = if rtl { (3, 1) } else { (1, 3) };
    let outer = [t.borders[0].clone(), t.borders[l].clone(), t.borders[2].clone(), t.borders[r].clone()];
    TableLayout { x0, cols, rows, bottom_border, spacing: t.cell_spacing, outer, id: TableId(t.id), fit, rtl }
}

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

// Row drawing
#[derive(Clone, Copy)]
pub struct CellPart<'a> {
    pub units: &'a [StoryUnit],
    pub base: f64,
    pub ends: bool,
}

impl CellPart<'_> {
    pub fn height(&self) -> f64 {
        self.units.last().map_or(0.0, |l| (l.top - self.base) + l.height)
    }
}

pub fn whole(row: &RowBox) -> Vec<CellPart<'_>> {
    row.cells.iter().map(|c| CellPart { units: &c.units.units, base: 0.0, ends: true }).collect()
}

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

pub(super) fn clip_x(children: &Rc<[Child]>, lo: f64, hi: f64, bottom: f64) -> Option<Rc<[Child]>> {
    let inside = |r: &GlyphRun, g: &(u32, f64), dx: f64, dy: f64| {
        let k = r.size / r.face.upem;
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
    let nested: Vec<Option<Child>> = children.iter().map(|c| clip_nested(c, lo, hi, bottom)).collect();
    if !children.iter().any(|c| matches!(c, Child::Line { placed, .. } if spills(placed))) && nested.iter().all(Option::is_none) {
        return None;
    }
    let clipped = children.iter().zip(nested).map(|(c, n)| match (c, n) {
        (_, Some(n)) => n,
        (Child::Line { para, placed }, None) if spills(placed) => {
            let (dx, dy) = (placed.x, placed.y);
            let items: Vec<Item> = placed
                .line
                .items
                .iter()
                .filter_map(|i| match i {
                    Item::Glyphs(r) => {
                        let glyphs: Box<[(u32, f64)]> = r.glyphs.iter().copied().filter(|g| inside(r, g, dx, dy)).collect();
                        let (from, to) = glyphs.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), g| (a.min(g.1), b.max(g.1)));
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

fn drawn_edges<'a>(tb: &TableLayout, c: &'a CellBox) -> [&'a Option<Border>; 4] {
    let [t, l, b, r] = &*c.edges;
    if tb.rtl { [t, r, b, l] } else { [t, l, b, r] }
}

pub fn draw_row(tb: &TableLayout, ri: usize, left: f64, top: f64, height: f64, parts: Option<&[CellPart]>, last_of_table: bool) -> RowFragment {
    let row = &tb.rows[ri];
    let whole = (height - row.height).abs() < EPS;
    let (lead, trail) = if whole { (row.lead, row.trail) } else { (0.0, 0.0) };
    let (outer_top, outer_bottom) = (top, top + height);
    let (top, height) = (top + lead, height - lead - trail);
    let x0 = left + tb.x0;
    let (mut background, mut cells, mut borders) = (Vec::new(), Vec::new(), Vec::new());
    let mut edges: Vec<(f64, usize)> = Vec::new();
    let mut rect = [x0, top, x0, top + height];
    if let (Some(mut first), Some(mut end)) = (row.cells.first(), row.cells.last()) {
        if tb.rtl {
            (first, end) = (end, first);
        }
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
    if let Some(fill) = row.shading {
        background.push(Item::rect(rect, fill));
    }
    for (k, c) in row.cells.iter().enumerate() {
        let part = parts.map(|p| p[k]).unwrap_or(CellPart { units: &c.units.units, base: 0.0, ends: true });
        let ch = if part.ends && part.base == 0.0 && part.units.len() == c.units.units.len() { c.align_h } else { part.height() };
        let height = height + c.below;
        let inner = height - row.top_border - c.margins[0] - c.margins[2];
        let off = match c.v_align.as_str() {
            "center" => ((inner - ch) / 2.0).max(0.0),
            "bottom" => (inner - ch).max(0.0),
            _ => 0.0,
        };
        let base_y = top + row.top_border + c.margins[0] + off;
        let base_x = x0 + c.x + c.cx;
        let below_margin = row.cells.iter().map(|c| c.margins[2]).fold(0.0, f64::max);
        let clip = if row.exact { top + height - below_margin } else { f64::INFINITY };
        let mut content = Vec::new();
        let mut hidden = Vec::new();
        for u in part.units {
            let ut = u.top - part.base;
            let bottom = clip - (base_y + ut) + INK_SLACK;
            let hide = match ink_top(&u.content) {
                Some(top) => top >= bottom - EPS,
                None => base_y + ut >= clip - EPS,
            };
            if hide {
                source_units(&u.content, &mut hidden);
                continue;
            }
            let children = clip_x(&u.content, -c.cx, c.w - c.cx, bottom).unwrap_or_else(|| u.content.clone());
            let children = unaligned_floats(&children, off, &c.fixed_floats).unwrap_or(children);
            content.push(Child::Group { offset: (base_x, base_y + ut), children });
        }
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
        if let Some(spacing) = row.spacing {
            let ncols = tb.cols.len();
            let (mut lo, mut ro) = (c.col == 0, c.col + c.span >= ncols);
            if tb.rtl {
                (lo, ro) = (ro, lo);
            }
            let bx0 = x0 + c.x + gap(spacing, lo);
            let bx1 = x0 + c.x + c.w - gap(spacing, ro);
            let (by0, by1) = (top + row.gap_top, top + height - gap(spacing, tb.is_last(ri)));
            if let Some(fill) = c.shading {
                background.push(Item::rect([bx0, by0, bx1, by1], fill));
            }
            let [t, l, b, r] = drawn_edges(tb, c);
            let vis = |e: &'_ Option<Border>| e.as_ref().filter(|b| b.visible()).map(|b| (thick(b), b.clone()));
            if let Some((w, e)) = vis(t) {
                edge(&mut borders, [bx0, by0, bx1, by0 + w], true, false, &e);
            }
            if let Some((w, e)) = vis(b) {
                edge(&mut borders, [bx0, by1 - w, bx1, by1], true, true, &e);
            }
            if let Some((w, e)) = vis(l) {
                edge(&mut borders, [bx0, by0, bx0 + w, by1], false, false, &e);
            }
            if let Some((w, e)) = vis(r) {
                edge(&mut borders, [bx1 - w, by0, bx1, by1], false, true, &e);
            }
            continue;
        }
        if let Some(fill) = c.shading {
            background.push(Item::rect([x0 + c.x, top, x0 + c.x + c.w, top + height], fill));
        }
        // Borders
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
        if let Some(t) = t.as_ref().filter(|b| b.visible() && c.v_merge != VMerge::Continue) {
            edge(&mut borders, [cx0 - bw(l) / 2.0, top, cx1 + bw(r) / 2.0, top + thick(t)], true, false, t);
        }
        if let Some(l) = l.as_ref().filter(|b| b.visible()) {
            edge(&mut borders, [cx0 - thick(l) / 2.0, top, cx0 + thick(l) / 2.0, top + height], false, false, l);
        }
        if let Some(r) = r.as_ref().filter(|b| b.visible()) {
            edge(&mut borders, [cx1 - thick(r) / 2.0, top, cx1 + thick(r) / 2.0, top + height], false, true, r);
        }
        if (last_of_table || row.closes)
            && let Some(b) = b.as_ref().filter(|b| b.visible())
        {
            edge(&mut borders, [cx0 - bw(l) / 2.0, top + height, cx1 + bw(r) / 2.0, top + height + thick(b)], true, true, b);
        }
    }
    for (a, b, s) in &row.seams {
        edge(&mut borders, [x0 + a, top, x0 + b, top + thick(s)], true, false, s);
    }
    if row.spacing.is_some_and(|s| s > 0.0) {
        // Outer border
        let [t, l, b, r] = &tb.outer;
        let x1 = x0 + tb.cols.iter().sum::<f64>();
        let (lw, rw) = (bw(l), bw(r));
        let bottom = top + height + if last_of_table { tb.bottom_border - bw(b) } else { 0.0 };
        if ri == 0
            && let Some(t) = t.as_ref().filter(|b| b.visible())
        {
            edge(&mut borders, [x0 - lw / 2.0, top, x1 + rw / 2.0, top + thick(t)], true, false, t);
        }
        if let Some(l) = l.as_ref().filter(|b| b.visible()) {
            edge(&mut borders, [x0 - lw / 2.0, top, x0 + lw / 2.0, bottom], false, false, l);
        }
        if let Some(r) = r.as_ref().filter(|b| b.visible()) {
            edge(&mut borders, [x1 - rw / 2.0, top, x1 + rw / 2.0, bottom], false, true, r);
        }
        if last_of_table && let Some(b) = b.as_ref().filter(|b| b.visible()) {
            edge(&mut borders, [x0 - lw / 2.0, bottom, x1 + rw / 2.0, bottom + thick(b)], true, true, b);
        }
    }
    // Division boxes
    if whole && !row.div_boxes.is_empty() {
        let side = |b: &Option<Border>| b.as_ref().map_or(0.0, |b| b.space + thick(b));
        let n = row.div_boxes.len();
        let (mut l, mut r) = (rect[0], rect[2]);
        let mut extents = vec![(0.0, 0.0); n];
        for (k, d) in row.div_boxes.iter().enumerate().rev() {
            l -= side(&d.borders[1]);
            r += side(&d.borders[3]);
            extents[k] = (l, r);
        }
        let opening: f64 = row.div_boxes.iter().filter(|d| d.starts).map(|d| side(&d.borders[0])).sum();
        let mut y = top - opening;
        let mut tops = vec![outer_top; n];
        for (k, d) in row.div_boxes.iter().enumerate().filter(|(_, d)| d.starts) {
            tops[k] = y;
            if let Some(b) = &d.borders[0] {
                edge(&mut borders, [extents[k].0, y, extents[k].1, y + thick(b)], true, false, b);
            }
            y += side(&d.borders[0]);
        }
        let mut y = top + height;
        let mut bottoms = vec![outer_bottom; n];
        for (k, d) in row.div_boxes.iter().enumerate().rev().filter(|(_, d)| d.ends) {
            if let Some(b) = &d.borders[2] {
                y += b.space;
                edge(&mut borders, [extents[k].0, y, extents[k].1, y + thick(b)], true, true, b);
                y += thick(b);
            }
            bottoms[k] = y;
        }
        for (k, d) in row.div_boxes.iter().enumerate() {
            if let Some(b) = &d.borders[1] {
                edge(&mut borders, [extents[k].0, tops[k], extents[k].0 + thick(b), bottoms[k]], false, false, b);
            }
            if let Some(b) = &d.borders[3] {
                edge(&mut borders, [extents[k].1 - thick(b), tops[k], extents[k].1, bottoms[k]], false, true, b);
            }
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

// Row cache
#[derive(Debug)]
pub struct DrawnRow(RowFragment);

impl PartialEq for DrawnRow {
    fn eq(&self, o: &Self) -> bool {
        format!("{:?}", self.0) == format!("{:?}", o.0)
    }
}

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

pub struct RowProbe<'a> {
    tb: &'a TableLayout,
    ri: usize,
    at: [u64; 2],
    last: bool,
}

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

// Story units
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

pub(super) const INK_SLACK: f64 = 0.06;

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
        let b = Border { style: "dotted".into(), width: 1.0, space: 0.0, shadow: false, color: None };
        let mut out = Vec::new();
        edge(&mut out, [0.0, 0.0, 9.0, 1.0], true, false, &b);
        let xs: Vec<f64> = out.iter().map(|i| if let Item::Rect { rect, .. } = i { rect[0] } else { -1.0 }).collect();
        assert_eq!(xs, vec![0.0, 2.0, 4.0, 6.0, 8.0]);
    }

    #[test]
    fn every_border_style_draws_its_own_way() {
        let b = |style: &str| Border { style: style.into(), width: 1.0, space: 0.0, shadow: false, color: Some(0x336699) };
        let drawn = |style: &str, far: bool| {
            let mut out = Vec::new();
            edge(&mut out, [0.0, 0.0, 30.0, thick(&b(style))], true, far, &b(style));
            out.iter()
                .map(|i| if let Item::Rect { rect, colour } = i { (rect.map(|v| (v * 100.0).round() as i64), *colour) } else { ([0; 4], 0) })
                .collect::<Vec<_>>()
        };
        let styles = [
            "single",
            "double",
            "triple",
            "dotted",
            "dashed",
            "dotDash",
            "dotDotDash",
            "thinThickSmallGap",
            "thickThinSmallGap",
            "thinThickThinSmallGap",
            "wave",
            "doubleWave",
            "threeDEmboss",
            "threeDEngrave",
            "outset",
            "inset",
        ];
        let all: Vec<_> = styles.iter().map(|s| drawn(s, false)).collect();
        for (i, a) in all.iter().enumerate() {
            for (j, b) in all.iter().enumerate().skip(i + 1) {
                assert_ne!(a, b, "{} and {} draw alike", styles[i], styles[j]);
            }
        }
        let ys: std::collections::BTreeSet<i64> = drawn("wave", false).iter().map(|(r, _)| r[1]).collect();
        assert!(ys.len() > 3, "a wave goes up and down");
        assert_ne!(drawn("outset", false), drawn("outset", true), "outset is light on one side and dark on the other");
        assert_eq!(drawn("outset", false), drawn("inset", true));
    }

    #[test]
    fn a_3d_border_is_three_lines_in_light_middle_and_dark() {
        let b = |style: &str| Border { style: style.into(), width: 1.0, space: 0.0, shadow: false, color: Some(0) };
        let drawn = |style: &str| {
            let mut out = Vec::new();
            edge(&mut out, [0.0, 0.0, 10.0, 2.5], true, false, &b(style));
            out.iter().map(|i| if let Item::Rect { rect, colour, .. } = i { (rect[3] - rect[1], *colour) } else { (0.0, 0) }).collect::<Vec<_>>()
        };
        assert_eq!(drawn("threeDEmboss"), vec![(0.75, 0xC0C0C0), (1.0, 0x808080), (0.75, 0x000000)]);
        assert_eq!(drawn("threeDEngrave"), vec![(0.75, 0x000000), (1.0, 0x808080), (0.75, 0xC0C0C0)]);
        assert_eq!(from_hls(to_hls(0x808080).0, to_hls(0x808080).1, to_hls(0x808080).2), 0x808080);
    }
}
