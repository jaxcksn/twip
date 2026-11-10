//! The rulers: their model for the selection, in points, the unit's marks, and what a point on a drawn ruler is
//! over. The engine draws the built-in rulers from this alone (`render::ruler`); an application drawing its own reads
//! the same model.

use crate::api::catalog::ruler as words;
use crate::api::services::{
    ClientRect, HorizontalRuler, MeasureDetail, RulerIndents, RulerModel, RulerUnit, RulerView, Span, TabSelector, VerticalRuler,
};
use crate::api::value::{Points, TabAlignment};
use crate::docx::Section;
use crate::editing::Editor;
use crate::editing::position::StoryId;
use crate::editing::view::{RulerMeasure, Tone};

/// A ruler band's thickness, CSS px: the paper strip and the desk on both sides of it.
pub const BAND: f64 = 18.0;

/// The desk between a band's edges and its paper strip, CSS px.
pub const INSET: f64 = 2.0;

/// Below the horizontal ruler's band, CSS px: a quiet drag's chip's top and a drag's dimension line.
const CHIP_Y: f64 = 38.0;
const DIMENSION_Y: f64 = 51.0;

/// How far below the band a dragged tab stop is removed, CSS px.
const REMOVE_BELOW: f64 = 30.0;

/// The tab type box in the rulers' corner, CSS px.
pub const SELECTOR: ClientRect = ClientRect { left: INSET, top: INSET, width: BAND - 2.0 * INSET, height: BAND - 2.0 * INSET };

/// A section's text columns between `left` and `right` (page points).
fn columns(s: &Section, left: f64, right: f64) -> Vec<Span> {
    let c = &s.columns;
    let n = c.num.max(1);
    let widths: Vec<(f64, f64)> = if c.equal || c.cols.len() < n {
        let w = ((right - left) - c.space * (n - 1) as f64) / n as f64;
        vec![(w, c.space); n]
    } else {
        c.cols[..n].to_vec()
    };
    let mut x = left;
    widths
        .into_iter()
        .map(|(w, space)| {
            let span = Span { start: Points(x), end: Points(x + w) };
            x += w + space;
            span
        })
        .collect()
}

/// The ruler model for the selection, in `section`: its page, the text area around the caret (the margins, its
/// text column, or its cell), the first selected paragraph's indents and stops, and the caret's table on its page.
pub fn model(e: &mut Editor, section: usize) -> RulerModel {
    let Some(s) = e.document().sections.get(section) else { return RulerModel::default() };
    // shortcut: mirrored margins and right-to-left sections show the left-to-right margins.
    let left = s.left + if e.document().settings.gutter_at_top { 0.0 } else { s.gutter };
    let right = s.page_w - s.right;
    let columns = columns(s, left, right);
    let (page_w, page_h, top, bottom) = (s.page_w, s.page_h, s.top, s.page_h - s.bottom);
    let paras = e.ruler_paras();
    let styles: Vec<_> = paras.iter().take(2000).filter_map(|&p| e.index().para(e.document(), p)).map(|p| p.style.clone()).collect();
    let key = |s: &crate::docx::ParaStyle| (s.ind_left, s.ind_right, s.ind_first, s.tabs.clone());
    let mixed = styles.first().is_some_and(|f| styles.iter().any(|s| key(s) != key(f)));
    let indents = styles.first().map_or(RulerIndents::default(), |s| RulerIndents {
        left: Points(s.ind_left),
        right: Points(s.ind_right),
        first_line: Points(s.ind_first),
    });
    let tabs = paras.first().map_or_else(Vec::new, |&p| e.tab_stops(p));

    // The text area: the caret's cell, else its text column in the body, else the margins.
    let mut text = Span { start: Points(left), end: Points(right) };
    let (mut table_columns, mut table_rows) = (Vec::new(), Vec::new());
    let focus = e.selection().focus();
    let caret = e.caret_view();
    let level = focus.and_then(|f| e.index().chain_of(&f).last().copied());
    match (caret, level) {
        (Some(c), Some(l)) => {
            if let Some(geo) = e.page_geometry(c.page) {
                let margins = e.index().table(e.document(), l.table).and_then(|t| t.rows.get(l.row)?.cells.get(l.cell)).map(|c| c.margins);
                if let (Some(cell), Some(m)) = (geo.cells.iter().find(|g| g.chain.last() == Some(&(l.table, l.row, l.cell))), margins) {
                    text = Span { start: Points(cell.rect[0] + m[1]), end: Points(cell.rect[2] - m[3]) };
                }
                let mut cells: Vec<[f64; 4]> =
                    geo.cells.iter().filter(|g| g.chain.last().is_some_and(|k| (k.0, k.1) == (l.table, l.row))).map(|g| g.rect).collect();
                cells.sort_by(|a, b| a[0].total_cmp(&b[0]));
                // Cells are laid out from the left border's middle; the boundaries are the grid's, from its outer edge.
                let table = e.index().table(e.document(), l.table);
                let border = table.and_then(|t| t.rows.get(l.row)?.cells.first()?.left.as_ref().or(t.borders[1].as_ref()));
                let shift = -border.filter(|b| b.visible()).map_or(0.0, |b| b.width / 2.0);
                table_columns = cells.iter().map(|r| r[0]).chain(cells.last().map(|r| r[2])).map(|x| Points(x + shift)).collect();
                let mut ys: Vec<f64> = geo.rows.iter().filter(|r| r.table == l.table).flat_map(|r| [r.rect[1], r.rect[3]]).collect();
                ys.sort_by(f64::total_cmp);
                ys.dedup_by(|a, b| (*a - *b).abs() < 0.01);
                table_rows = ys.into_iter().map(Points).collect();
            }
        }
        (Some(c), None) if focus.is_some_and(|f| f.story == StoryId::Body) => {
            let x = Points(c.x);
            if let Some(col) = columns.iter().min_by(|a, b| dist(a, x).total_cmp(&dist(b, x))) {
                text = *col;
            }
        }
        _ => {}
    }
    RulerModel {
        mixed,
        horizontal: HorizontalRuler {
            page_width: Points(page_w),
            text,
            indents,
            tabs,
            default_tab_stop: Points(e.document().settings.default_tab),
            columns,
            table_columns,
        },
        vertical: VerticalRuler { page_height: Points(page_h), text: Span { start: Points(top), end: Points(bottom) }, table_rows },
    }
}

/// How far `x` is outside a span (0 inside it).
fn dist(s: &Span, x: Points) -> f64 {
    (s.start.0 - x.0).max(x.0 - s.end.0).max(0.0)
}

/// The unit a language's rulers show by default: inches in the United States, centimetres elsewhere.
pub fn default_unit(language: &str) -> RulerUnit {
    if language.eq_ignore_ascii_case("en-US") { RulerUnit::Inches } else { RulerUnit::Centimeters }
}

/// A unit's labelled marks and its minor marks (what drags snap to), and how many units a labelled mark is, in
/// points.
pub fn marks(unit: RulerUnit) -> (f64, f64, f64) {
    const CM: f64 = 72.0 / 2.54;
    match unit {
        RulerUnit::Inches => (72.0, 9.0, 1.0),
        RulerUnit::Centimeters => (CM, CM / 4.0, 1.0),
        RulerUnit::Millimeters => (CM, CM / 10.0, 10.0),
        RulerUnit::Points => (72.0, 6.0, 72.0),
        RulerUnit::Picas => (72.0, 6.0, 6.0),
    }
}

/// The step drags and clicks snap to, in points: a sixteenth of an inch, a quarter centimetre, a millimetre, or the
/// minor mark.
pub fn step(unit: RulerUnit) -> f64 {
    match unit {
        RulerUnit::Inches => 4.5,
        RulerUnit::Centimeters | RulerUnit::Millimeters | RulerUnit::Points | RulerUnit::Picas => marks(unit).1,
    }
}

/// `v` snapped to the unit's step counted from `origin`.
pub fn snap(v: f64, origin: f64, unit: RulerUnit) -> f64 {
    let step = step(unit);
    origin + ((v - origin) / step).round() * step
}

/// A length in the unit with its decimals and no suffix: `0.50`.
pub fn number(pt: f64, unit: RulerUnit) -> String {
    let cm = 72.0 / 2.54;
    match unit {
        RulerUnit::Inches => format!("{:.2}", pt / 72.0),
        RulerUnit::Centimeters => format!("{:.2}", pt / cm),
        RulerUnit::Millimeters => format!("{:.1}", pt / cm * 10.0),
        RulerUnit::Points => format!("{pt:.1}"),
        RulerUnit::Picas => format!("{:.2}", pt / 12.0),
    }
}

/// A length in the unit with its suffix: `0.50″`, `1.27 cm`.
pub fn length(pt: f64, unit: RulerUnit) -> String {
    let suffix = match unit {
        RulerUnit::Inches => "″",
        RulerUnit::Centimeters => " cm",
        RulerUnit::Millimeters => " mm",
        RulerUnit::Points => " pt",
        RulerUnit::Picas => " pi",
    };
    number(pt, unit) + suffix
}

/// A change in the unit, signed: `+0.25″`, `−0.25″`.
pub(crate) fn change(pt: f64, unit: RulerUnit) -> String {
    format!("{}{}", if pt >= 0.0 { '+' } else { '−' }, length(pt.abs(), unit))
}

/// Which rulers show: they sit along the viewport's top and left edges.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Bands {
    pub horizontal: bool,
    pub vertical: bool,
}

/// Something on a ruler a drag moves.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Marker {
    /// The tab stop at this position.
    Tab(f64),
    FirstLine,
    /// The hanging indent: the left indent, the first line staying where it is.
    Hanging,
    /// The left indent with the first line.
    Left,
    Right,
    /// The left and right margins.
    TextStart,
    TextEnd,
    /// The boundary after text column `k`.
    Column(u32),
    /// The caret's row's cell boundary `k`.
    TableColumn(u32),
    /// The top and bottom margins.
    Top,
    Bottom,
    /// The caret's table's row boundary `k`.
    TableRow(u32),
}

impl Marker {
    /// Whether it moves along the horizontal ruler.
    pub fn horizontal(self) -> bool {
        !matches!(self, Marker::Top | Marker::Bottom | Marker::TableRow(_))
    }
}

impl Marker {
    /// Where it is now along its ruler, page points (none: the model has no such marker).
    pub fn position(self, m: &RulerModel) -> Option<f64> {
        let (h, v) = (&m.horizontal, &m.vertical);
        let (start, left) = (h.text.start.0, h.indents.left.0);
        Some(match self {
            Marker::Tab(p) => start + p,
            Marker::FirstLine => start + left + h.indents.first_line.0,
            Marker::Hanging | Marker::Left => start + left,
            Marker::Right => h.text.end.0 - h.indents.right.0,
            Marker::TextStart => start,
            Marker::TextEnd => h.text.end.0,
            Marker::Column(k) => h.columns.get(k as usize)?.end.0,
            Marker::TableColumn(k) => h.table_columns.get(k as usize)?.0,
            Marker::Top => v.text.start.0,
            Marker::Bottom => v.text.end.0,
            Marker::TableRow(k) => v.table_rows.get(k as usize)?.0,
        })
    }

    /// What it's called, what it measures with it at `at` (page points along its ruler), and the span measured.
    fn reading(self, m: &RulerModel, at: f64) -> Option<(String, f64, (f64, f64))> {
        let (h, v) = (&m.horizontal, &m.vertical);
        let (start, end) = (h.text.start.0, h.text.end.0);
        let name = |n: &str| n.to_string();
        Some(match self {
            Marker::Tab(p) => {
                let stop = h.tabs.iter().find(|t| (t.position.0 - p).abs() < 0.5)?;
                (format!("{} {}", tab_name(stop.alignment), words::TAB), at - start, (start, at))
            }
            Marker::FirstLine => (name(words::FIRST_LINE), at - start, (start, at)),
            Marker::Hanging => (name(words::HANGING), at - start, (start, at)),
            Marker::Left => (name(words::LEFT_INDENT), at - start, (start, at)),
            Marker::Right => (name(words::RIGHT_INDENT), end - at, (at, end)),
            Marker::TextStart => (name(words::LEFT_MARGIN), at, (0.0, at)),
            Marker::TextEnd => (name(words::RIGHT_MARGIN), h.page_width.0 - at, (at, h.page_width.0)),
            Marker::Column(k) => {
                let from = h.columns.get(k as usize)?.start.0;
                (name(words::COLUMN_WIDTH), at - from, (from, at))
            }
            Marker::TableColumn(k) => {
                let from = h.table_columns.get((k as usize).checked_sub(1)?)?.0;
                (name(words::COLUMN_WIDTH), at - from, (from, at))
            }
            Marker::Top => (name(words::TOP_MARGIN), at, (0.0, at)),
            Marker::Bottom => (name(words::BOTTOM_MARGIN), v.page_height.0 - at, (at, v.page_height.0)),
            Marker::TableRow(k) => {
                let from = v.table_rows.get((k as usize).checked_sub(1)?)?.0;
                (name(words::ROW_HEIGHT), at - from, (from, at))
            }
        })
    }
}

/// A tab alignment's name: `Left`.
fn tab_name(a: TabAlignment) -> &'static str {
    words::tab_type(match a {
        TabAlignment::Left => TabSelector::Left,
        TabAlignment::Center => TabSelector::Center,
        TabAlignment::Right => TabSelector::Right,
        TabAlignment::Decimal => TabSelector::Decimal,
        TabAlignment::Bar => TabSelector::Bar,
    })
    .0
}

/// Whether a tab stop dragged to viewport `y` (CSS px) is removed on release: more than the removal distance below
/// the band.
pub fn removing(y: f64) -> bool {
    y > BAND + REMOVE_BELOW
}

/// Where a click on the empty ruler at `pt` (page points) adds a stop: snapped unless `free`, and inside the text
/// area (none at or past its edges).
pub fn ghost(view: &RulerView, pt: f64, free: bool) -> Option<f64> {
    let (start, end) = (view.model.horizontal.text.start.0, view.model.horizontal.text.end.0);
    let at = if free { pt } else { snap(pt, start, view.unit) };
    (at > start && at < end).then_some(at)
}

/// A tab stop's mark on the horizontal ruler, CSS px.
pub fn tab_anchor(view: &RulerView, position: f64) -> ClientRect {
    let x = view.horizontal.origin + (view.model.horizontal.text.start.0 + position) * view.horizontal.scale;
    ClientRect { left: x - 7.0, top: 10.0, width: 14.0, height: 14.0 }
}

/// What a drag of `marker` to `at` (page points along its ruler) measures: a dimension line from where the value is
/// counted to the marker, labelled with its name and value, and with Alt or at the full level the change since the
/// press; at the quiet level a chip with the value only. A tab stop being removed says so instead. A table column
/// border measures the columns either side, over the table.
pub fn drag_measures(view: &RulerView, marker: Marker, at: f64, removing: bool, detail: MeasureDetail, alt: bool) -> Vec<RulerMeasure> {
    let (m, unit) = (&view.model, view.unit);
    let full = detail == MeasureDetail::Full || (detail == MeasureDetail::Standard && alt);
    let quiet = detail == MeasureDetail::Quiet;
    let (h, v) = (view.horizontal, view.vertical);
    if let Marker::TableColumn(k) = marker {
        let b = &m.horizontal.table_columns;
        let k = k as usize;
        let top = m.vertical.table_rows.first().map_or(BAND + DIMENSION_Y, |t| v.origin + t.0 * v.scale);
        let widths = [k.checked_sub(1).and_then(|i| b.get(i)).map(|l| (l.0, at)), b.get(k + 1).map(|r| (at, r.0))];
        if quiet {
            let (a, z) = widths.into_iter().flatten().next().unwrap_or((at, at));
            return vec![RulerMeasure::Chip { at: [h.origin + at * h.scale + 8.0, top - 24.0], label: length(z - a, unit) }];
        }
        let y = (top - 12.0 - v.origin) / v.scale.max(1e-9);
        return widths
            .into_iter()
            .flatten()
            .map(|(a, z)| RulerMeasure::Dimension { from: [a, y], to: [z, y], label: number(z - a, unit), tone: Tone::Accent })
            .collect();
    }
    let Some((name, value, (a, z))) = marker.reading(m, at) else { return Vec::new() };
    let was = marker.position(m).and_then(|p| marker.reading(m, p)).map_or(value, |r| r.1);
    let horizontal = marker.horizontal();
    let guide = if horizontal { h.origin + at * h.scale } else { v.origin + at * v.scale };
    if quiet {
        let label = if removing { words::REMOVE.to_string() } else { format!("{name} {}", length(value.abs(), unit)) };
        let at = if horizontal { [guide + 8.0, BAND + CHIP_Y] } else { [BAND + CHIP_Y, guide + 8.0] };
        return vec![RulerMeasure::Chip { at, label }];
    }
    let label = if removing {
        words::RELEASE_TO_REMOVE.to_string()
    } else {
        let delta = value - was;
        let shows = full && number(delta.abs(), unit) != number(0.0, unit);
        let change = if shows { format!("  Δ {}", change(delta, unit)) } else { String::new() };
        format!("{name} {}{change}", length(value.abs(), unit))
    };
    let (from, to) = if horizontal {
        let y = (BAND + DIMENSION_Y - v.origin) / v.scale.max(1e-9);
        ([a, y], [z, y])
    } else {
        let x = (BAND + DIMENSION_Y - h.origin) / h.scale.max(1e-9);
        ([x, a], [x, z])
    };
    vec![RulerMeasure::Dimension { from, to, label, tone: Tone::Accent }]
}

/// What hovering `on` says, for the host's tooltip: the marker's name, its value and what a click or drag does; over
/// the empty ruler, where a click adds the box's type (`free`: Alt is down, no snapping). With the box it is about,
/// viewport CSS px.
pub fn hint(view: &RulerView, on: Hit, selector: TabSelector, free: bool) -> Option<(ClientRect, String)> {
    let (m, unit) = (&view.model, view.unit);
    let (h, v) = (view.horizontal, view.vertical);
    let along = |x: f64| ClientRect { left: x - 7.0, top: 0.0, width: 14.0, height: BAND };
    match on {
        Hit::Ruler => None,
        Hit::Selector => Some((SELECTOR, format!("{} {} — {}", words::TAB_TYPE, words::tab_type(selector).0, words::TAB_TYPE_DOES))),
        Hit::Text(pt) => {
            let at = ghost(view, pt, free)?;
            let name = words::tab_type(selector).0.to_lowercase();
            let what = match selector {
                TabSelector::FirstLineIndent | TabSelector::HangingIndent => format!("{} {name}", words::CLICK_TO_SET),
                TabSelector::Left | TabSelector::Center | TabSelector::Right | TabSelector::Decimal | TabSelector::Bar => {
                    format!("{} {name} {}", words::CLICK_TO_ADD, words::TAB)
                }
            };
            Some((along(h.origin + at * h.scale), format!("{what} · {}", length(at - m.horizontal.text.start.0, unit))))
        }
        Hit::Marker(marker) => {
            let at = marker.position(m)?;
            let reading = || marker.reading(m, at).map(|(name, value, _)| format!("{name} · {}", length(value.abs(), unit)));
            let label = match marker {
                Marker::FirstLine => format!("{} — {}", reading()?, words::FIRST_LINE_DOES),
                Marker::Hanging => format!("{} — {}", reading()?, words::HANGING_DOES),
                Marker::Left => format!("{} — {}", reading()?, words::LEFT_INDENT_DOES),
                Marker::Right => reading()?,
                Marker::Tab(_) => format!("{} — {}", reading()?, words::TAB_DOES),
                Marker::TextStart | Marker::TextEnd | Marker::Top | Marker::Bottom => format!("{} — {}", reading()?, words::MARGIN_DOES),
                Marker::Column(_) => words::COLUMN_SPACING.to_string(),
                Marker::TableColumn(_) => words::COLUMN_BORDER.to_string(),
                Marker::TableRow(_) => words::ROW_BORDER.to_string(),
            };
            let rect = if marker.horizontal() {
                along(h.origin + at * h.scale)
            } else {
                ClientRect { left: 0.0, top: v.origin + at * v.scale - 7.0, width: BAND, height: 14.0 }
            };
            Some((rect, label))
        }
    }
}

/// What a point on the rulers is over.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hit {
    /// The tab selector box in the corner.
    Selector,
    Marker(Marker),
    /// An empty spot of the horizontal ruler's text area, at this page x.
    Text(f64),
    /// Elsewhere on a ruler.
    Ruler,
}

/// What the viewport point (`x`, `y`, CSS px) is over on the rulers shown, if they're there.
pub fn hit(view: &RulerView, bands: Bands, x: f64, y: f64) -> Option<Hit> {
    let (h, v, m) = (&view.horizontal, &view.vertical, &view.model);
    let on_h = bands.horizontal && (0.0..BAND).contains(&y);
    let on_v = bands.vertical && (0.0..BAND).contains(&x);
    if on_h && x < BAND {
        return Some(Hit::Selector);
    }
    let near = |px: f64, pt: f64, by: f64| (px - (h.origin + pt * h.scale)).abs() <= by;
    if on_h {
        let r = &m.horizontal;
        let (start, end) = (r.text.start.0, r.text.end.0);
        let left = start + r.indents.left.0;
        let f = y / BAND;
        if f < 0.5 && near(x, left + r.indents.first_line.0, 6.0) {
            return Some(Hit::Marker(Marker::FirstLine));
        }
        if f >= 0.5 && near(x, left, 6.0) {
            return Some(Hit::Marker(if f < 0.75 { Marker::Hanging } else { Marker::Left }));
        }
        if f >= 0.5 && near(x, end - r.indents.right.0, 6.0) {
            return Some(Hit::Marker(Marker::Right));
        }
        if let Some(t) = r.tabs.iter().find(|t| near(x, start + t.position.0, 4.0)) {
            return Some(Hit::Marker(Marker::Tab(t.position.0)));
        }
        if let Some(k) = r.table_columns.iter().position(|b| near(x, b.0, 4.0)) {
            return Some(Hit::Marker(Marker::TableColumn(k as u32)));
        }
        if let Some(k) = r.columns.windows(2).position(|c| near(x, c[0].end.0, 3.0) || near(x, c[1].start.0, 3.0)) {
            return Some(Hit::Marker(Marker::Column(k as u32)));
        }
        if r.table_columns.is_empty() {
            if near(x, start, 4.0) {
                return Some(Hit::Marker(Marker::TextStart));
            }
            if near(x, end, 4.0) {
                return Some(Hit::Marker(Marker::TextEnd));
            }
        }
        let pt = (x - h.origin) / h.scale;
        return Some(if (start..=end).contains(&pt) { Hit::Text(pt) } else { Hit::Ruler });
    }
    if on_v {
        let near = |px: f64, pt: f64| (px - (v.origin + pt * v.scale)).abs() <= 3.0;
        let r = &m.vertical;
        if let Some(k) = r.table_rows.iter().position(|b| near(y, b.0)) {
            return Some(Hit::Marker(Marker::TableRow(k as u32)));
        }
        if r.table_rows.is_empty() {
            if near(y, r.text.start.0) {
                return Some(Hit::Marker(Marker::Top));
            }
            if near(y, r.text.end.0) {
                return Some(Hit::Marker(Marker::Bottom));
            }
        }
        return Some(Hit::Ruler);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::services::{RulerAxis, TabStopInfo};

    fn view() -> RulerView {
        let mut m = RulerModel::default();
        m.horizontal.text = Span { start: Points(72.0), end: Points(540.0) };
        m.horizontal.indents.left = Points(36.0);
        m.horizontal.indents.first_line = Points(18.0);
        m.horizontal.tabs = vec![TabStopInfo { position: Points(144.0), ..TabStopInfo::default() }];
        m.vertical.text = Span { start: Points(72.0), end: Points(720.0) };
        RulerView {
            model: m,
            page: 1,
            horizontal: RulerAxis { origin: 30.0, scale: 1.0 },
            vertical: RulerAxis { origin: 30.0, scale: 1.0 },
            ..RulerView::default()
        }
    }

    #[test]
    fn a_point_on_the_rulers_names_what_it_is_over() {
        let (v, b) = (view(), Bands { horizontal: true, vertical: true });
        assert_eq!(hit(&v, b, 5.0, 5.0), Some(Hit::Selector));
        assert_eq!(hit(&v, b, 30.0 + 126.0, 2.0), Some(Hit::Marker(Marker::FirstLine)));
        assert_eq!(hit(&v, b, 30.0 + 108.0, 10.0), Some(Hit::Marker(Marker::Hanging)));
        assert_eq!(hit(&v, b, 30.0 + 108.0, 15.0), Some(Hit::Marker(Marker::Left)));
        assert_eq!(hit(&v, b, 30.0 + 540.0, 12.0), Some(Hit::Marker(Marker::Right)));
        assert_eq!(hit(&v, b, 30.0 + 216.0, 12.0), Some(Hit::Marker(Marker::Tab(144.0))));
        assert_eq!(hit(&v, b, 30.0 + 72.0, 10.0), Some(Hit::Marker(Marker::TextStart)));
        assert_eq!(hit(&v, b, 30.0 + 300.0, 10.0), Some(Hit::Text(300.0)));
        assert_eq!(hit(&v, b, 30.0 + 20.0, 10.0), Some(Hit::Ruler));
        assert_eq!(hit(&v, b, 10.0, 30.0 + 720.0), Some(Hit::Marker(Marker::Bottom)));
        assert_eq!(hit(&v, b, 300.0, 300.0), None);
        assert_eq!(hit(&v, Bands::default(), 300.0, 10.0), None);
    }

    /// The tabs fixture with the caret at the start of body paragraph `n`, and the model there.
    fn at(n: usize) -> (Editor, RulerModel) {
        use crate::editing::tests::{fonts, open, para_id};
        let mut e = open("FX-tabs.docx");
        e.complete(fonts());
        let p = para_id(&e, n);
        e.select_range(p, crate::layout::Unit(0), crate::layout::Unit(0), fonts());
        let section = e.format_state().map_or(0, |f| f.section);
        let m = model(&mut e, section);
        (e, m)
    }

    fn span(a: f64, b: f64) -> Span {
        Span { start: Points(a), end: Points(b) }
    }

    #[test]
    fn the_model_in_the_body_shows_the_margins_indents_and_stops() {
        let (_, m) = at(0);
        let h = &m.horizontal;
        assert_eq!((h.page_width, h.text, h.columns.clone()), (Points(612.0), span(72.0, 540.0), vec![span(72.0, 540.0)]));
        let tabs: Vec<(f64, bool)> = h.tabs.iter().map(|t| (t.position.0, t.inherited)).collect();
        assert_eq!(tabs, [(72.0, true), (288.0, true)]);
        assert_eq!(h.default_tab_stop, Points(36.0));
        assert!(h.table_columns.is_empty() && !m.mixed);
        let (_, m) = at(1);
        let i = m.horizontal.indents;
        assert_eq!((i.left, i.right, i.first_line), (Points(36.0), Points(18.0), Points(18.0)));
        assert_eq!(m.horizontal.tabs.iter().map(|t| (t.position.0, t.inherited)).collect::<Vec<_>>(), [(216.0, false)]);
    }

    #[test]
    fn the_model_in_a_cell_shows_the_cell_and_the_rows_boundaries() {
        let (_, m) = at(4);
        let h = &m.horizontal;
        assert_eq!(h.table_columns, [72.0, 228.0, 384.0, 540.0].map(Points));
        // The cell's text area is inside its margins (5.4 pt each side).
        assert!((h.text.start.0 - 233.65).abs() < 0.5 && (h.text.end.0 - 378.85).abs() < 0.5, "{:?}", h.text);
        let v = &m.vertical;
        assert_eq!((v.page_height, v.text), (Points(792.0), span(72.0, 720.0)));
        assert_eq!(v.table_rows.len(), 2, "the one row's top and bottom: {:?}", v.table_rows);
        assert!(v.table_rows[0].0 > 72.0 && v.table_rows[1].0 > v.table_rows[0].0);
    }

    #[test]
    fn the_model_in_a_second_text_column_shows_that_column() {
        let (mut e, _) = at(7);
        let last = crate::editing::tests::texts(&e).len() - 1;
        let p = crate::editing::tests::para_id(&e, last);
        e.select_range(p, crate::layout::Unit(0), crate::layout::Unit(0), crate::editing::tests::fonts());
        let m = model(&mut e, 1);
        assert_eq!(m.horizontal.columns, [span(72.0, 288.0), span(324.0, 540.0)]);
        assert_eq!(m.horizontal.text, span(324.0, 540.0));
        let (_, m) = at(7);
        assert_eq!(m.horizontal.text, span(72.0, 288.0));
    }

    #[test]
    fn the_model_in_a_header_shows_the_margins() {
        let (mut e, _) = at(0);
        assert!(matches!(
            e.execute(crate::editing::commands::Command::EditHeader, crate::editing::tests::fonts()),
            crate::editing::commands::Outcome::Changed(_)
        ));
        let m = model(&mut e, 0);
        assert_eq!(m.horizontal.text, span(72.0, 540.0));
        assert!(m.horizontal.table_columns.is_empty() && m.vertical.table_rows.is_empty());
    }

    #[test]
    fn drags_snap_to_a_sixteenth_inch_or_a_quarter_centimetre() {
        assert_eq!(snap(72.0 + 40.0, 72.0, RulerUnit::Inches), 72.0 + 40.5);
        assert_eq!(snap(72.0 + 19.44, 72.0, RulerUnit::Inches), 72.0 + 18.0, "0.27 in snaps to 0.25 in");
        assert!((snap(100.0, 0.0, RulerUnit::Centimeters) - 72.0 / 2.54 * 3.5).abs() < 1e-9);
        assert_eq!(default_unit("en-US"), RulerUnit::Inches);
        assert_eq!(default_unit("de-DE"), RulerUnit::Centimeters);
    }

    #[test]
    fn lengths_read_in_the_ruler_unit_with_two_decimals() {
        assert_eq!(length(36.0, RulerUnit::Inches), "0.50″");
        assert_eq!(length(36.0, RulerUnit::Centimeters), "1.27 cm");
        assert_eq!(number(144.0, RulerUnit::Inches), "2.00");
        assert_eq!(change(-18.0, RulerUnit::Inches), "−0.25″");
        assert_eq!(change(36.0, RulerUnit::Centimeters), "+1.27 cm");
        assert!(!removing(BAND + 30.0) && removing(BAND + 31.0), "more than 30 px below the band");
    }

    /// [`view`] on a letter page, with a table of three columns whose row is 100 to 130 pt down.
    fn page() -> RulerView {
        let mut v = view();
        v.model.horizontal.page_width = Points(612.0);
        v.model.vertical.page_height = Points(792.0);
        v
    }

    fn labels(m: &[RulerMeasure]) -> Vec<&str> {
        m.iter()
            .map(|m| match m {
                RulerMeasure::Dimension { label, .. } | RulerMeasure::Chip { label, .. } | RulerMeasure::Status { label, .. } => label.as_str(),
            })
            .collect()
    }

    #[test]
    fn a_drag_measures_from_the_margin_to_the_marker() {
        let v = page();
        let (s, q, f) = (MeasureDetail::Standard, MeasureDetail::Quiet, MeasureDetail::Full);
        let d = |m, at, removing, detail, alt| drag_measures(&v, m, at, removing, detail, alt);
        // The dimension line is 51 px below the band, from the text start to the marker.
        assert_eq!(
            d(Marker::FirstLine, 108.0, false, s, false),
            [RulerMeasure::Dimension { from: [72.0, 39.0], to: [108.0, 39.0], label: "First line 0.50″".into(), tone: Tone::Accent }]
        );
        // Alt, or the full level, adds the change since the press (the first line was 0.75 in).
        assert_eq!(labels(&d(Marker::FirstLine, 108.0, false, s, true)), ["First line 0.50″  Δ −0.25″"]);
        assert_eq!(labels(&d(Marker::FirstLine, 108.0, false, f, false)), ["First line 0.50″  Δ −0.25″"]);
        assert_eq!(labels(&d(Marker::FirstLine, 126.0, false, f, false)), ["First line 0.75″"], "no change, no Δ");
        assert_eq!(labels(&d(Marker::Hanging, 90.0, false, f, false)), ["Hanging indent 0.25″  Δ −0.25″"]);
        assert_eq!(labels(&d(Marker::Right, 532.8, false, s, true)), ["Right indent 0.10″  Δ +0.10″"]);
        assert_eq!(labels(&d(Marker::Tab(144.0), 270.0, false, s, true)), ["Left tab 2.75″  Δ +0.75″"]);
        assert_eq!(labels(&d(Marker::TextStart, 90.0, false, s, false)), ["Left margin 1.25″"]);
        assert_eq!(
            d(Marker::TextEnd, 540.0, false, s, false),
            [RulerMeasure::Dimension { from: [540.0, 39.0], to: [612.0, 39.0], label: "Right margin 1.00″".into(), tone: Tone::Accent }]
        );
        // A stop dragged off the ruler says what the release does.
        assert_eq!(labels(&d(Marker::Tab(144.0), 270.0, true, s, false)), ["Release to remove tab stop"]);
        // Quiet: a chip by the guide with the value only.
        assert_eq!(
            d(Marker::FirstLine, 108.0, false, q, true),
            [RulerMeasure::Chip { at: [30.0 + 108.0 + 8.0, BAND + 38.0], label: "First line 0.50″".into() }]
        );
        assert_eq!(labels(&d(Marker::Tab(144.0), 270.0, true, q, false)), ["Remove tab stop"]);
        // In centimetres.
        let mut cm = page();
        cm.unit = RulerUnit::Centimeters;
        assert_eq!(labels(&drag_measures(&cm, Marker::FirstLine, 99.0, false, s, true)), ["First line 0.95 cm  Δ −0.95 cm"]);
        assert_eq!(labels(&drag_measures(&cm, Marker::Tab(144.0), 288.0, false, s, false)), ["Left tab 7.62 cm"]);
    }

    #[test]
    fn a_table_column_drag_measures_the_columns_either_side_over_the_table() {
        let mut v = page();
        v.model.horizontal.table_columns = [72.0, 228.0, 384.0, 540.0].map(Points).to_vec();
        v.model.vertical.table_rows = [100.0, 130.0].map(Points).to_vec();
        // 12 px over the table's top.
        let y = 30.0 + 100.0 - 12.0 - 30.0;
        assert_eq!(
            drag_measures(&v, Marker::TableColumn(1), 216.0, false, MeasureDetail::Standard, false),
            [
                RulerMeasure::Dimension { from: [72.0, y], to: [216.0, y], label: "2.00".into(), tone: Tone::Accent },
                RulerMeasure::Dimension { from: [216.0, y], to: [384.0, y], label: "2.33".into(), tone: Tone::Accent },
            ]
        );
        assert_eq!(labels(&drag_measures(&v, Marker::TableColumn(0), 90.0, false, MeasureDetail::Standard, false)), ["1.92"]);
        assert_eq!(
            drag_measures(&v, Marker::TableColumn(1), 216.0, false, MeasureDetail::Quiet, false),
            [RulerMeasure::Chip { at: [30.0 + 216.0 + 8.0, 30.0 + 100.0 - 24.0], label: "2.00″".into() }]
        );
    }

    #[test]
    fn hovering_a_marker_names_it_with_its_value_and_what_it_does() {
        let v = page();
        let h = |on| hint(&v, Hit::Marker(on), TabSelector::Left, false);
        let text = |m: Option<(ClientRect, String)>| m.expect("a hint").1;
        assert_eq!(
            h(Marker::FirstLine),
            Some((
                ClientRect { left: 30.0 + 126.0 - 7.0, top: 0.0, width: 14.0, height: BAND },
                "First line · 0.75″ — indents only the first line".into()
            ))
        );
        assert_eq!(text(h(Marker::Hanging)), "Hanging indent · 0.50″ — where wrapped lines start");
        assert_eq!(text(h(Marker::Left)), "Left indent · 0.50″ — moves the first line with it");
        assert_eq!(text(h(Marker::Right)), "Right indent · 0.00″");
        assert_eq!(text(h(Marker::Tab(144.0))), "Left tab · 2.00″ — click for options, drag down to remove");
        assert_eq!(text(h(Marker::TextStart)), "Left margin · 1.00″ — drag to change");
        assert_eq!(text(h(Marker::TextEnd)), "Right margin · 1.00″ — drag to change");
        let mut table = page();
        table.model.horizontal.table_columns = vec![Points(72.0), Points(300.0)];
        assert_eq!(text(hint(&table, Hit::Marker(Marker::TableColumn(1)), TabSelector::Left, false)), "Column border — drag to resize");
        // A vertical ruler marker's box is across the vertical band.
        assert_eq!(
            h(Marker::Top),
            Some((ClientRect { left: 0.0, top: 30.0 + 72.0 - 7.0, width: BAND, height: 14.0 }, "Top margin · 1.00″ — drag to change".into()))
        );
        // Over the empty ruler: where a click adds the box's type, snapped unless Alt is down.
        let empty = |selector, free| text(hint(&v, Hit::Text(163.0), selector, free));
        assert_eq!(empty(TabSelector::Decimal, false), "Click to add a decimal tab · 1.25″");
        assert_eq!(empty(TabSelector::Decimal, true), "Click to add a decimal tab · 1.26″");
        assert_eq!(empty(TabSelector::FirstLineIndent, false), "Click to set the first line indent · 1.25″");
        assert_eq!(hint(&v, Hit::Text(73.0), TabSelector::Left, false), None, "no stop at the margin");
        // The box says its type.
        assert_eq!(hint(&v, Hit::Selector, TabSelector::Left, false), Some((SELECTOR, "Tab type: Left — click to change".into())));
        assert_eq!(hint(&v, Hit::Ruler, TabSelector::Left, false), None);
    }
}
