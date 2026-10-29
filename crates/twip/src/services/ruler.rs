//! The rulers: their model for the selection, in points, the unit's marks, and what a point on a drawn ruler is
//! over. The engine draws the built-in rulers from this alone (`render::ruler`); an application drawing its own reads
//! the same model.

use crate::api::services::{HorizontalRuler, RulerIndents, RulerModel, RulerUnit, RulerView, Span, VerticalRuler};
use crate::api::value::Points;
use crate::docx::Section;
use crate::editing::Editor;
use crate::editing::position::StoryId;

/// A ruler band's thickness, CSS px.
pub const BAND: f64 = 20.0;

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

/// `v` snapped to the unit's minor marks counted from `origin`.
pub fn snap(v: f64, origin: f64, unit: RulerUnit) -> f64 {
    let minor = marks(unit).1;
    origin + ((v - origin) / minor).round() * minor
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
        if f < 0.4 && near(x, left + r.indents.first_line.0, 5.0) {
            return Some(Hit::Marker(Marker::FirstLine));
        }
        if f >= 0.5 && near(x, left, 5.0) {
            return Some(Hit::Marker(if f < 0.75 { Marker::Hanging } else { Marker::Left }));
        }
        if f >= 0.5 && near(x, end - r.indents.right.0, 5.0) {
            return Some(Hit::Marker(Marker::Right));
        }
        if let Some(t) = r.tabs.iter().find(|t| near(x, start + t.position.0, 4.0)) {
            return Some(Hit::Marker(Marker::Tab(t.position.0)));
        }
        if let Some(k) = r.table_columns.iter().position(|b| near(x, b.0, 3.0)) {
            return Some(Hit::Marker(Marker::TableColumn(k as u32)));
        }
        if let Some(k) = r.columns.windows(2).position(|c| near(x, c[0].end.0, 3.0) || near(x, c[1].start.0, 3.0)) {
            return Some(Hit::Marker(Marker::Column(k as u32)));
        }
        if r.table_columns.is_empty() {
            if near(x, start, 3.0) {
                return Some(Hit::Marker(Marker::TextStart));
            }
            if near(x, end, 3.0) {
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
        assert_eq!(hit(&v, b, 30.0 + 108.0, 12.0), Some(Hit::Marker(Marker::Hanging)));
        assert_eq!(hit(&v, b, 30.0 + 108.0, 18.0), Some(Hit::Marker(Marker::Left)));
        assert_eq!(hit(&v, b, 30.0 + 540.0, 15.0), Some(Hit::Marker(Marker::Right)));
        assert_eq!(hit(&v, b, 30.0 + 216.0, 15.0), Some(Hit::Marker(Marker::Tab(144.0))));
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
    fn drags_snap_to_the_minor_marks() {
        assert_eq!(snap(72.0 + 40.0, 72.0, RulerUnit::Inches), 72.0 + 36.0);
        assert!((snap(100.0, 0.0, RulerUnit::Centimeters) - 72.0 / 2.54 * 3.5).abs() < 1e-9);
        assert_eq!(default_unit("en-US"), RulerUnit::Inches);
        assert_eq!(default_unit("de-DE"), RulerUnit::Centimeters);
    }
}
