//! Table formatting commands: cell alignment, shading, borders and margins; row heights, can't split
//! and header rows; column widths; the table's width, alignment, indent, style and style options. Cell and row
//! commands apply to the target's cells (a cell block, or the caret's cell) of the innermost table; each command is
//! one step.

use crate::docx::props::{Border, Jc, Look, Width};
use crate::font::Fonts;

use super::Editor;
use super::commands::Outcome;
use super::step::{Step, TableProps};
use super::table::Grid;

#[derive(Clone, Debug, PartialEq)]
pub enum TableFormat {
    /// `top`, `center` or `bottom`.
    CellAlign(String),
    /// A fill (RGB), or none.
    CellShading(Option<u32>),
    /// Borders on some edges of the target's cells; `None` with [`Edges::None`] takes them all off.
    CellBorders(Edges, Border),
    /// Top, left, bottom and right margins, in points.
    CellMargins([f64; 4]),
    /// Repeat Header Rows on for the table's rows from the first through the target's last, or off for every row.
    HeaderRow(bool),
    /// The target rows' height and rule (`atLeast`, `exact`); `None`: auto.
    RowHeight(Option<(f64, String)>),
    /// Allow Row to Break Across Pages off (`true`) or on for the target rows.
    RowCantSplit(bool),
    /// The target columns' width, in points: the other columns keep theirs, the table grows or shrinks.
    ColumnWidth(f64),
    TableWidth(Width),
    TableAlign(Jc),
    /// The table's indent from the margin, in points.
    TableIndent(f64),
    /// A table style by id: the cells, rows and paragraphs take its formatting again.
    TableStyle(String),
    /// Table Style Options (header row, total row, first and last column, banded rows and columns).
    TableLook(Look),
}

/// Which edges of a cell block a border goes on (the Borders menu's choices).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Edges {
    All,
    Outside,
    Inside,
    InsideH,
    InsideV,
    Top,
    Bottom,
    Left,
    Right,
    /// No Border: every edge of the block's cells.
    None,
}

/// "No border", written as `nil` so that it overrides the table's and the style's borders.
fn nil() -> Border {
    Border { style: "nil".into(), width: 0.0, space: 0.0, color: None }
}

fn border_xml(w: &str, name: &str, b: &Border) -> String {
    if !b.visible() {
        return format!("<{w}{name} {w}val=\"nil\"/>");
    }
    let color = b.color.map_or("auto".to_string(), |c| format!("{c:06X}"));
    format!(
        "<{w}{name} {w}val=\"{}\" {w}sz=\"{}\" {w}space=\"{}\" {w}color=\"{color}\"/>",
        crate::docx::write::props::esc(&b.style),
        (b.width * 8.0).round() as i64,
        b.space.round() as i64
    )
}

impl Editor {
    pub(super) fn table_format(&mut self, f: &TableFormat, fonts: &Fonts) -> Outcome {
        let Some(tg) = self.table_target() else { return Outcome::Unchanged("not in a table") };
        self.table_format_on(tg, f, fonts)
    }

    /// [`Editor::table_format`] on an explicit target.
    pub(super) fn table_format_on(&mut self, tg: super::tables::Target, f: &TableFormat, fonts: &Fonts) -> Outcome {
        let Some(t) = self.index.table(&self.doc, tg.table) else { return Outcome::Unchanged("no table") };
        let g = Grid::of(t);
        let mut rows = t.rows.clone();
        let mut grid = None;
        let mut props = TableProps::of(t);
        let mut restyle = false;
        let cx = self.cx();
        let w = cx.w.clone();
        // A direct property in the cell's `w:tcPr` (re-resolution keeps it over the style region).
        let set_direct = |cell: &mut crate::docx::Cell, local: &str, xml: String| {
            let mut p = crate::docx::write::props::Props::parse(cell.direct.0.as_deref(), "tcPr", &cx);
            p.set(local, xml);
            cell.direct = crate::docx::Raw(Some(std::rc::Rc::from(p.xml().unwrap_or_else(|| format!("<{w}tcPr/>")).as_str())));
        };
        let in_block = |r: usize, c: usize, s: usize| (tg.rows.0..=tg.rows.1).contains(&r) && c + s.max(1) > tg.cols.0 && c <= tg.cols.1;
        let (r0, r1) = (tg.rows.0, tg.rows.1.min(rows.len().saturating_sub(1)));
        match f {
            TableFormat::CellAlign(_) | TableFormat::CellShading(_) | TableFormat::CellMargins(_) => {
                for (r, row) in rows.iter_mut().enumerate() {
                    for (k, cell) in row.cells.iter_mut().enumerate() {
                        let Some(&(c, s, _)) = g.rows[r].get(k) else { continue };
                        if !in_block(r, c, s) {
                            continue;
                        }
                        match f {
                            TableFormat::CellAlign(v) => {
                                cell.v_align = v.clone();
                                set_direct(cell, "vAlign", format!("<{w}vAlign {w}val=\"{}\"/>", crate::docx::write::props::esc(v)));
                            }
                            TableFormat::CellShading(fill) => {
                                cell.shading = *fill;
                                let f = fill.map_or("auto".to_string(), |f| format!("{f:06X}"));
                                set_direct(cell, "shd", format!("<{w}shd {w}val=\"clear\" {w}color=\"auto\" {w}fill=\"{f}\"/>"));
                            }
                            TableFormat::CellMargins(m) => {
                                cell.margins = *m;
                                let tw = |v: f64| (v * 20.0).round() as i64;
                                set_direct(
                                    cell,
                                    "tcMar",
                                    format!(
                                        "<{w}tcMar><{w}top {w}w=\"{}\" {w}type=\"dxa\"/><{w}left {w}w=\"{}\" {w}type=\"dxa\"/><{w}bottom {w}w=\"{}\" {w}type=\"dxa\"/><{w}right {w}w=\"{}\" {w}type=\"dxa\"/></{w}tcMar>",
                                        tw(m[0]),
                                        tw(m[1]),
                                        tw(m[2]),
                                        tw(m[3])
                                    ),
                                );
                            }
                            _ => {}
                        }
                    }
                }
            }
            TableFormat::CellBorders(edges, b) => {
                // Each side of each block cell: whether it is on the block's outside, and whether it gets the border.
                let (c0, c1) = tg.cols;
                let want = |outside: bool, horizontal: bool, side: Edges| match edges {
                    Edges::All | Edges::None => true,
                    Edges::Outside => outside,
                    Edges::Inside => !outside,
                    Edges::InsideH => !outside && horizontal,
                    Edges::InsideV => !outside && !horizontal,
                    e => outside && *e == side,
                };
                let value = if *edges == Edges::None { nil() } else { b.clone() };
                // Shared edges: a block side on its outside sets the neighbouring cell's facing side too.
                let mut shared: Vec<(usize, usize, usize)> = Vec::new(); // (row, cell, side: 0 top 1 left 2 bottom 3 right)
                let mut touched: Vec<(usize, usize)> = Vec::new();
                for r in r0..=r1 {
                    for (k, &(c, s, _)) in g.rows[r].iter().enumerate() {
                        if !in_block(r, c, s) {
                            continue;
                        }
                        let last = c + s.max(1) - 1;
                        let sides = [
                            (r == r0, true, Edges::Top),
                            (c <= c0, false, Edges::Left),
                            (r == r1, true, Edges::Bottom),
                            (last >= c1, false, Edges::Right),
                        ];
                        let cell = &mut rows[r].cells[k];
                        for (i, (outside, horizontal, side)) in sides.into_iter().enumerate() {
                            if !want(outside, horizontal, side) {
                                continue;
                            }
                            *[&mut cell.top, &mut cell.left, &mut cell.bottom, &mut cell.right][i] = Some(value.clone());
                            if outside {
                                let overlaps = |row: usize| {
                                    g.rows
                                        .get(row)
                                        .into_iter()
                                        .flatten()
                                        .enumerate()
                                        .filter(move |(_, (oc, os, _))| *oc + (*os).max(1) > c && *oc <= last)
                                        .map(|(j, _)| j)
                                };
                                match i {
                                    0 if r > 0 => shared.extend(overlaps(r - 1).map(|j| (r - 1, j, 2))),
                                    2 => shared.extend(overlaps(r + 1).map(|j| (r + 1, j, 0))),
                                    1 if k > 0 => shared.push((r, k - 1, 3)),
                                    3 if k + 1 < g.rows[r].len() => shared.push((r, k + 1, 1)),
                                    _ => {}
                                }
                            }
                        }
                        touched.push((r, k));
                    }
                }
                for (r, k, side) in shared {
                    let cell = &mut rows[r].cells[k];
                    *[&mut cell.top, &mut cell.left, &mut cell.bottom, &mut cell.right][side] = Some(value.clone());
                    touched.push((r, k));
                }
                touched.sort_unstable();
                touched.dedup();
                for (r, k) in touched {
                    let cell = &mut rows[r].cells[k];
                    let side = |name: &str, b: &Option<Border>| b.as_ref().map_or(String::new(), |b| border_xml(&w, name, b));
                    let x = format!(
                        "<{w}tcBorders>{}{}{}{}</{w}tcBorders>",
                        side("top", &cell.top),
                        side("left", &cell.left),
                        side("bottom", &cell.bottom),
                        side("right", &cell.right)
                    );
                    set_direct(cell, "tcBorders", x);
                }
            }
            &TableFormat::HeaderRow(on) => {
                rows[..=r1].iter_mut().for_each(|r| r.header = on);
                if !on {
                    rows.iter_mut().for_each(|r| r.header = false);
                }
            }
            TableFormat::RowHeight(h) => {
                let h = h.as_ref().map(|(v, rule)| ((v * 20.0).round() / 20.0, rule.clone()));
                rows[r0..=r1].iter_mut().for_each(|r| r.height = h.clone());
            }
            &TableFormat::RowCantSplit(on) => {
                rows[r0..=r1].iter_mut().for_each(|r| r.cant_split = on);
            }
            TableFormat::ColumnWidth(v) => {
                let v = ((v.max(1.0)) * 20.0).round() / 20.0;
                let mut gr = t.grid.clone();
                for c in tg.cols.0..=tg.cols.1.min(gr.len().saturating_sub(1)) {
                    gr[c] = v;
                }
                for (r, row) in rows.iter_mut().enumerate() {
                    for (k, cell) in row.cells.iter_mut().enumerate() {
                        if let Some(&(c, s, _)) = g.rows[r].get(k) {
                            cell.width = Some(Width::Dxa((c..c + s.max(1)).filter_map(|i| gr.get(i)).sum()));
                        }
                    }
                }
                if matches!(props.width, Width::Dxa(_)) {
                    props.width = Width::Dxa(gr.iter().sum());
                }
                grid = Some(gr);
            }
            TableFormat::TableWidth(v) => props.width = *v,
            TableFormat::TableAlign(j) => props.jc = *j,
            TableFormat::TableIndent(v) => props.ind = (v * 20.0).round() / 20.0,
            TableFormat::TableStyle(_) | TableFormat::TableLook(_) => {
                let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
                if let TableFormat::TableStyle(id) = f {
                    if !res.styles.by_id.contains_key(id) {
                        return Outcome::Unchanged("no such style");
                    }
                    props.style_id = Some(id.clone());
                    // The table's own properties again: the new style's, its direct ones over them.
                    let mut tbl = res.styles.table_region(Some(id), &[]).tbl.clone();
                    let direct =
                        t.direct.0.as_deref().and_then(|raw| cx.parse(raw, |_, n| crate::docx::props::TblProps::parse(Some(n)))).unwrap_or_default();
                    tbl.merge(&direct);
                    let b = &tbl.borders;
                    props.borders = [b.top.clone(), b.left.clone(), b.bottom.clone(), b.right.clone(), b.inside_h.clone(), b.inside_v.clone()];
                    props.bands = (tbl.row_band.unwrap_or(0), tbl.col_band.unwrap_or(0));
                    if direct.look.is_none() && props.look.is_none() {
                        props.look = tbl.look;
                    }
                }
                if let TableFormat::TableLook(l) = f {
                    props.look = Some(*l);
                }
                // Every cell is re-resolved from its region at its place (cells made without properties too).
                for c in rows.iter_mut().flat_map(|r| r.cells.iter_mut()) {
                    if c.direct.0.is_none() {
                        c.direct = crate::docx::Raw(Some(std::rc::Rc::from(format!("<{w}tcPr/>").as_str())));
                    }
                }
                restyle = true;
            }
        }
        if rows == t.rows && grid.is_none() && props == TableProps::of(t) {
            return Outcome::Unchanged("already so");
        }
        let n = rows.len();
        let props_changed = props != TableProps::of(t);
        let rows_changed = rows != t.rows || grid.is_some();
        let mut tx = self.begin("Table Properties");
        let ok = (!props_changed || self.step(&mut tx, Step::TableProps { table: tg.table, props }).is_ok())
            && (!rows_changed || self.step(&mut tx, Step::Rows { table: tg.table, range: 0..n, with: rows, grid }).is_ok());
        if !ok {
            self.abort(tx);
            return Outcome::Unchanged("table edit failed");
        }
        if restyle
            || matches!(f, TableFormat::CellShading(_) | TableFormat::CellAlign(_) | TableFormat::CellBorders(..) | TableFormat::CellMargins(_))
        {
            tx.stale_tables.push(tg.table);
        }
        self.commit(tx, None, fonts)
    }
}
