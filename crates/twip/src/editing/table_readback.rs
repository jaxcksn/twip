//! Table read-back

use crate::api::state::Tri;
use crate::api::value::{HorizontalAlignment, NoneOr, Points};
use crate::docx::props::{Border, Jc, TblProps, Width};
use crate::docx::{Cell, VMerge};
use crate::font::Fonts;

use super::Editor;
use super::commands::Outcome;
use super::readback::{BorderLine, ShadingValue};
use super::state::fold;
use super::table::Grid;
use super::table_format::TableFormat;

/// How a table's columns are sized.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableFit {
    /// Fixed column widths (Word's Fixed Column Width).
    Fixed,
    /// Columns fitted to their contents (AutoFit Contents).
    Contents,
    /// The table fills the width it is in (AutoFit Window: a percentage width, fitted).
    Window,
}

/// The selected cells' edges, each the same line over them or mixed: a cell's own line, else the line its row's or
/// table's borders give that edge.
#[derive(Clone, Debug, PartialEq)]
pub struct CellEdgeLines {
    /// The selection's top edge.
    pub top: Tri<NoneOr<BorderLine>>,
    /// The selection's left edge.
    pub left: Tri<NoneOr<BorderLine>>,
    /// The selection's bottom edge.
    pub bottom: Tri<NoneOr<BorderLine>>,
    /// The selection's right edge.
    pub right: Tri<NoneOr<BorderLine>>,
    /// The lines between the selected rows (unavailable with one row).
    pub inside_horizontal: Tri<NoneOr<BorderLine>>,
    /// The lines between the selected columns (unavailable with one column).
    pub inside_vertical: Tri<NoneOr<BorderLine>>,
    /// The diagonal from top left to bottom right.
    pub diagonal_down: Tri<NoneOr<BorderLine>>,
    /// The diagonal from top right to bottom left.
    pub diagonal_up: Tri<NoneOr<BorderLine>>,
}

/// A table's (or a row's exceptions') borders: its outside edges and inside lines.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TableEdgeLines {
    /// The top edge.
    pub top: Option<BorderLine>,
    /// The left edge.
    pub left: Option<BorderLine>,
    /// The bottom edge.
    pub bottom: Option<BorderLine>,
    /// The right edge.
    pub right: Option<BorderLine>,
    /// The lines between rows.
    pub inside_horizontal: Option<BorderLine>,
    /// The lines between columns.
    pub inside_vertical: Option<BorderLine>,
}

impl TableEdgeLines {
    fn of(b: &[Option<Border>; 6]) -> TableEdgeLines {
        let l = |k: usize| b[k].as_ref().and_then(BorderLine::of);
        TableEdgeLines { top: l(0), left: l(1), bottom: l(2), right: l(3), inside_horizontal: l(4), inside_vertical: l(5) }
    }
}

/// Cell margins on each side.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SideMargins<T = Points> {
    /// The top margin.
    pub top: T,
    /// The left margin.
    pub left: T,
    /// The bottom margin.
    pub bottom: T,
    /// The right margin.
    pub right: T,
}

impl SideMargins {
    fn of(m: [f64; 4]) -> SideMargins {
        SideMargins { top: Points(m[0]), left: Points(m[1]), bottom: Points(m[2]), right: Points(m[3]) }
    }
}

/// The selected rows' exceptions to their table's properties, each the same over them or mixed; `None` for a row that
/// takes the table's.
#[derive(Clone, Debug, PartialEq)]
pub struct RowExceptionsReadback {
    /// Whether the rows carry exceptions at all.
    pub any: Tri<bool>,
    /// The rows' own alignment.
    pub alignment: Tri<Option<HorizontalAlignment>>,
    /// The rows' own indent from the left edge.
    pub indent: Tri<Option<Points>>,
    /// The rows' own space between cells.
    pub cell_spacing: Tri<Option<Points>>,
    /// The rows' own shading.
    pub shading: Tri<NoneOr<ShadingValue>>,
    /// The rows' own borders.
    pub borders: Tri<Option<TableEdgeLines>>,
}

/// The selection's table beyond [`crate::api::state::TableSelection`]: borders, shading, margins, row exceptions, the
/// table's layout and the current cell's merge.
#[derive(Clone, Debug, PartialEq)]
pub struct TableReadback {
    /// The selected cells' edges.
    pub cell_borders: CellEdgeLines,
    /// The table's own borders (the whole table's edges and inside lines).
    pub table_borders: TableEdgeLines,
    /// The selected cells' shading: fill, pattern and pattern colour.
    pub cell_shading: Tri<NoneOr<ShadingValue>>,
    /// The table's shading (behind spaced cells).
    pub table_shading: Option<ShadingValue>,
    /// The selected cells' margins on each side.
    pub cell_margins: SideMargins<Tri<Points>>,
    /// The table's default cell margins (Table Options).
    pub default_cell_margins: SideMargins,
    /// Whether the selected cells' end marks are hidden from row height.
    pub hide_mark: Tri<bool>,
    /// The selected rows' exceptions.
    pub row_exceptions: RowExceptionsReadback,
    /// How the table's columns are sized.
    pub fit: TableFit,
    /// A positioned table: whether other positioned tables may overlap it; `None` for an inline table.
    pub allow_overlap: Option<bool>,
    /// The columns the current cell spans.
    pub column_span: u32,
    /// The rows the current cell spans (more than one: merged down).
    pub row_span: u32,
}

fn alignment(j: Jc) -> HorizontalAlignment {
    match j {
        Jc::Center => HorizontalAlignment::Center,
        Jc::Right => HorizontalAlignment::Right,
        _ => HorizontalAlignment::Left,
    }
}

fn line(b: &Option<Border>) -> NoneOr<BorderLine> {
    b.as_ref().and_then(BorderLine::of).map_or(NoneOr::None, NoneOr::Value)
}

/// The rows `row` spans from `(r, k)`: itself and the cells continuing it below.
fn rows_spanned(g: &Grid, r: usize, k: usize) -> usize {
    let col = g.col_of(r, k).unwrap_or(0);
    let mut end = r;
    while let Some(n) = g.cell_at(end + 1, col)
        && g.rows[end + 1][n].0 == col
        && g.continues(end + 1, n)
    {
        end += 1;
    }
    end - r + 1
}

fn edges(e: &TableEdgeLines) -> crate::api::state::TableBorderLines {
    let b = |l: &Option<BorderLine>| l.clone().map(Into::into);
    crate::api::state::TableBorderLines {
        top: b(&e.top),
        left: b(&e.left),
        bottom: b(&e.bottom),
        right: b(&e.right),
        inside_horizontal: b(&e.inside_horizontal),
        inside_vertical: b(&e.inside_vertical),
    }
}

fn none_or<T>(v: Option<T>) -> NoneOr<T> {
    v.map_or(NoneOr::None, NoneOr::Value)
}

impl TableReadback {
    /// The table state's fields this read-back fills (the rest default).
    pub fn into_state(self) -> crate::api::state::TableSelection {
        use crate::api::state::{CellBorderLines, CellMarginsState, RowExceptionsState, TableCellMarginsState, TableSelection};
        let c = |t: Tri<NoneOr<BorderLine>>| {
            t.map(|v| match v {
                NoneOr::Value(l) => NoneOr::Value(l.into()),
                NoneOr::None => NoneOr::None,
            })
        };
        let x = self.row_exceptions;
        let m = self.default_cell_margins;
        TableSelection {
            cell_borders: CellBorderLines {
                top: c(self.cell_borders.top),
                left: c(self.cell_borders.left),
                bottom: c(self.cell_borders.bottom),
                right: c(self.cell_borders.right),
                inside_horizontal: c(self.cell_borders.inside_horizontal),
                inside_vertical: c(self.cell_borders.inside_vertical),
                diagonal_down: c(self.cell_borders.diagonal_down),
                diagonal_up: c(self.cell_borders.diagonal_up),
            },
            table_borders: edges(&self.table_borders),
            cell_shading_value: self.cell_shading,
            table_shading: self.table_shading,
            cell_margins: CellMarginsState {
                top: self.cell_margins.top,
                left: self.cell_margins.left,
                bottom: self.cell_margins.bottom,
                right: self.cell_margins.right,
            },
            default_cell_margins: TableCellMarginsState { top: m.top, left: m.left, bottom: m.bottom, right: m.right },
            hide_mark: self.hide_mark,
            row_exceptions: RowExceptionsState {
                any: x.any,
                alignment: x.alignment.map(none_or),
                indent: x.indent.map(none_or),
                cell_spacing: x.cell_spacing.map(none_or),
                shading: x.shading,
                borders: x.borders.map(|b| none_or(b.as_ref().map(edges))),
            },
            fit: match self.fit {
                TableFit::Fixed => crate::api::state::TableFit::Fixed,
                TableFit::Contents => crate::api::state::TableFit::Contents,
                TableFit::Window => crate::api::state::TableFit::Window,
            },
            column_span: self.column_span,
            row_span: self.row_span,
            ..TableSelection::default()
        }
    }
}

impl Editor {
    /// The selection's table beyond the table state ([`TableReadback`]); none outside a table.
    pub fn table_readback(&self) -> Option<TableReadback> {
        let tg = self.table_target()?;
        let t = self.index.table(&self.doc, tg.table)?;
        let g = Grid::of(t);
        let last_row = t.rows.len().checked_sub(1)?;
        let (r0, r1) = (tg.rows.0.min(last_row), tg.rows.1.min(last_row));
        let (c0, c1) = tg.cols;
        // The selected cells: row, index in the row, first column and span; a merged cell once, at its first row.
        let cells: Vec<(usize, usize, usize, usize)> = (r0..=r1)
            .flat_map(|r| g.rows[r].iter().enumerate().map(move |(k, &(c, s, v))| (r, k, c, s.max(1), v)))
            .filter(|&(r, _, c, s, v)| c + s > c0 && c <= c1 && (v != VMerge::Continue || r == r0))
            .map(|(r, k, c, s, _)| {
                let (r, k) = g.merge_start(r, k);
                (r, k, c, s)
            })
            .collect();
        let cell = |r: usize, k: usize| -> &Cell { &t.rows[r].cells[k] };
        let cols = g.cols;
        // A cell's edge `side` (top, left, bottom, right): its own, else its row's or table's line there.
        let edge = |r: usize, k: usize, c: usize, s: usize, side: usize| -> Option<Border> {
            let end = r + rows_spanned(&g, r, k) - 1;
            let row_of = if side == 2 { end } else { r };
            let tb = crate::layout::table::row_borders(t, row_of);
            let own = match side {
                0 => &cell(r, k).top,
                1 => &cell(r, k).left,
                2 => g.cell_at(end, c).map_or(&cell(r, k).bottom, |n| &t.rows[end].cells[n].bottom),
                _ => &cell(r, k).right,
            };
            let outer = match side {
                0 => r == 0,
                1 => c == 0,
                2 => end == last_row,
                _ => c + s >= cols,
            };
            let table = match (side, outer) {
                (0, true) => 0,
                (1, true) => 1,
                (2, true) => 2,
                (3, true) => 3,
                (0 | 2, false) => 4,
                _ => 5,
            };
            own.clone().or_else(|| tb[table].clone())
        };
        let ends = |r: usize, k: usize| r + rows_spanned(&g, r, k) - 1;
        // Where two selected cells meet: both cells' sides, as the edge is drawn.
        let settings = &self.doc.settings;
        let single = settings.compat() == crate::docx::Compat::Legacy && settings.policy.use_single_border_for_contiguous_cells;
        let (row_borders, shared_edge) = (crate::layout::table::row_borders, crate::layout::table::shared_edge);
        let gr = &g;
        let cell_borders = CellEdgeLines {
            top: fold(cells.iter().filter(|&&(r, ..)| r <= r0).map(|&(r, k, c, s)| line(&edge(r, k, c, s, 0)))),
            left: fold(cells.iter().filter(|&&(_, _, c, _)| c <= c0).map(|&(r, k, c, s)| line(&edge(r, k, c, s, 1)))),
            bottom: fold(cells.iter().filter(|&&(r, k, ..)| ends(r, k) >= r1).map(|&(r, k, c, s)| line(&edge(r, k, c, s, 2)))),
            right: fold(cells.iter().filter(|&&(_, _, c, s)| c + s > c1).map(|&(r, k, c, s)| line(&edge(r, k, c, s, 3)))),
            inside_horizontal: fold(cells.iter().filter(|&&(r, k, ..)| ends(r, k) < r1).flat_map(|&(r, k, c, s)| {
                let end = ends(r, k);
                let above = (gr.cell_at(end, c).map_or(&cell(r, k).bottom, |n| &t.rows[end].cells[n].bottom), &row_borders(t, end)[4]);
                gr.rows[end + 1]
                    .iter()
                    .enumerate()
                    .filter(move |&(_, &(bc, bs, _))| bc < c + s && bc + bs.max(1) > c)
                    .map(move |(n, _)| line(&shared_edge(above, (&t.rows[end + 1].cells[n].top, &row_borders(t, end + 1)[4]), single)))
            })),
            inside_vertical: fold(cells.iter().filter(|&&(_, _, c, s)| c + s <= c1).flat_map(|&(r, k, c, s)| {
                (r.max(r0)..=ends(r, k).min(r1)).filter_map(move |rr| {
                    let (a, b) = (gr.cell_at(rr, c)?, gr.cell_at(rr, c + s)?);
                    let (row, inside) = (&t.rows[rr], &row_borders(t, rr)[5]);
                    (a != b).then(|| line(&shared_edge((&row.cells[a].right, inside), (&row.cells[b].left, inside), single)))
                })
            })),
            diagonal_down: fold(cells.iter().map(|&(r, k, ..)| line(&cell(r, k).diagonals.as_deref().and_then(|d| d[0].clone())))),
            diagonal_up: fold(cells.iter().map(|&(r, k, ..)| line(&cell(r, k).diagonals.as_deref().and_then(|d| d[1].clone())))),
        };
        let margin = |side: usize| fold(cells.iter().map(|&(r, k, ..)| Points(cell(r, k).margins[side])));
        let res = self.doc.resolver.as_ref();
        let parse_tbl = |p: &crate::format::TableProps| -> TblProps {
            res.map_or_else(TblProps::default, |res| crate::docx::table_props::tbl_props(p, &res.cascade().colours))
        };
        let tbl = res
            .map(|res| {
                crate::docx::table_props::table_on(
                    &res.cascade().table_region(t.style_id.as_deref(), &[]).format.table,
                    t.direct.get(),
                    &res.cascade().colours,
                )
            })
            .unwrap_or_default();
        // A side neither the table nor its style sets: as cells resolve it.
        let unset = if t.style_id.is_none() { 0.5 } else { 0.0 };
        let m = &tbl.margins;
        let default_cell_margins =
            SideMargins::of([m.top.unwrap_or(0.0), m.left.unwrap_or(unset), m.bottom.unwrap_or(0.0), m.right.unwrap_or(unset)]);
        let rows = &t.rows[r0..=r1];
        let exceptions: Vec<TblProps> = rows.iter().map(|r| parse_tbl(r.exceptions.get())).collect();
        let row_exceptions = RowExceptionsReadback {
            any: fold(exceptions.iter().map(|x| {
                x.jc.is_some()
                    || x.ind.is_some()
                    || x.width.is_some()
                    || x.cell_spacing.is_some()
                    || x.shd.is_some()
                    || x.fixed.is_some()
                    || x.look.is_some()
                    || x.borders != crate::docx::props::Borders::default()
                    || x.margins != crate::docx::props::Margins::default()
            })),
            alignment: fold(exceptions.iter().map(|x| x.jc.map(alignment))),
            indent: fold(rows.iter().map(|r| r.ind.map(Points))),
            cell_spacing: fold(rows.iter().map(|r| r.cell_spacing.map(Points))),
            shading: fold(exceptions.iter().map(|x| x.shd.as_ref().and_then(ShadingValue::of).map_or(NoneOr::None, NoneOr::Value))),
            borders: fold(rows.iter().map(|r| r.borders.as_deref().map(TableEdgeLines::of))),
        };
        let focus = match self.selection {
            super::selection::Selection::Cells { focus, .. } => g.cell_at(focus.row, focus.col).map(|k| (focus.row, k)),
            _ => {
                self.selection.focus().and_then(|p| self.index.chain_of(&p).last().copied()).filter(|l| l.table == tg.table).map(|l| (l.row, l.cell))
            }
        };
        let (fr, fk) = focus.unwrap_or_else(|| (r0, g.cell_at(r0, c0).unwrap_or(0)));
        let (fr, fk) = g.merge_start(fr, fk);
        Some(TableReadback {
            cell_borders,
            table_borders: TableEdgeLines::of(&t.borders),
            cell_shading: fold(
                cells
                    .iter()
                    .map(|&(r, k, ..)| self.cell_shd(t, (r, k), cell(r, k)).as_ref().and_then(ShadingValue::of).map_or(NoneOr::None, NoneOr::Value)),
            ),
            table_shading: t.shd.as_ref().and_then(ShadingValue::of),
            cell_margins: SideMargins { top: margin(0), left: margin(1), bottom: margin(2), right: margin(3) },
            default_cell_margins,
            hide_mark: fold(cells.iter().map(|&(r, k, ..)| cell(r, k).hide_mark)),
            row_exceptions,
            fit: match (t.fixed, t.width) {
                (true, _) => TableFit::Fixed,
                (false, Width::Pct(p)) if (p - 1.0).abs() < 1e-9 => TableFit::Window,
                _ => TableFit::Contents,
            },
            allow_overlap: t.floating.map(|f| !f.never_overlap),
            column_span: g.rows.get(fr).and_then(|row| row.get(fk)).map_or(1, |&(_, s, _)| s.max(1)) as u32,
            row_span: rows_spanned(&g, fr, fk) as u32,
        })
    }

    /// Sets the table's default cell margins (Table Options): the margins of every cell without its own. Each side 0
    /// to 1584 pt; `None` keeps a side.
    pub fn set_table_cell_margins(&mut self, m: [Option<Points>; 4], fonts: &Fonts) -> Outcome {
        let mut v = [None; 4];
        for (k, side) in m.iter().enumerate() {
            v[k] = match side {
                Some(p) if !(p.0.is_finite() && (0.0..=1584.0).contains(&p.0)) => return Outcome::invalid("a cell margin is 0…1584 pt"),
                Some(p) => Some((p.0 * 20.0).round() / 20.0),
                None => None,
            };
        }
        if self.table_target().is_none() {
            return Outcome::unavailable("not in a table");
        }
        if v.iter().all(Option::is_none) {
            return Outcome::Unchanged("no side given");
        }
        if let Some(out) = self.refuse_like(&crate::api::command::Command::SetTableAlignment(Default::default())) {
            return out;
        }
        if self.enforced_protection().is_some_and(|p| p.formatting) {
            return Outcome::Refused(crate::api::error::Error::new(crate::api::error::ErrorCode::NotAllowed, "formatting is restricted"));
        }
        self.table_format(&TableFormat::TableCellMargins(v), fonts)
    }

    /// Sets whether other positioned tables may overlap the selection's positioned table (Word's "Allow overlap").
    pub fn set_table_overlap(&mut self, allow: bool, fonts: &Fonts) -> Outcome {
        let Some(tg) = self.table_target() else { return Outcome::unavailable("not in a table") };
        let Some(t) = self.index.table(&self.doc, tg.table) else { return Outcome::unavailable("not in a table") };
        let Some(mut pos) = t.floating else { return Outcome::unavailable("the table isn't positioned") };
        // Positioning isn't direct formatting: only read-only and Restrict Editing refuse it.
        if let Some(out) = self.refuse_like(&crate::api::command::Command::SetTablePosition(None)) {
            return out;
        }
        pos.never_overlap = !allow;
        self.set_floating(tg.table, Some(pos), fonts)
    }
}
