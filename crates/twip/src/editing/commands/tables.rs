//! Tables: commands on the selection's cells, rows, columns and table.

use crate::api::command::{AutoFit, BorderEdges, BorderStyle, CellBorders, CellMargins, HorizontalSide, InsertColumns, SplitCells, TableStyleChange};
use crate::api::value::{AutoOr, CellAlignment, HeightRule, HorizontalAlignment, RowHeight, StyleId, TableWidth};
use crate::docx::props::{Border, Jc, Width};
use crate::font::Fonts;

use super::Outcome;
use crate::editing::Editor;
use crate::editing::table::Grid;
use crate::editing::table_format::{Edges, TableFormat};

impl Editor {
    /// Columns left or right of the selection, `count` of them, as one step.
    pub(in crate::editing) fn insert_columns_by(&mut self, c: InsertColumns, fonts: &Fonts) -> Outcome {
        let right = c.side == HorizontalSide::Right;
        self.joined("Insert Columns", fonts, |e| {
            let mut out = Outcome::Unchanged("no table");
            for _ in 0..c.count.unwrap_or(1) {
                match e.insert_columns(right, fonts) {
                    o @ Outcome::Changed(_) => out = o,
                    o => return if matches!(out, Outcome::Changed(_)) { out } else { o },
                }
            }
            out
        })
    }

    /// The selected cells split into `columns` and then `rows`, as one step.
    pub(in crate::editing) fn split_cells(&mut self, s: SplitCells, fonts: &Fonts) -> Outcome {
        let (columns, rows) = (s.columns as usize, s.rows.unwrap_or(1) as usize);
        self.joined("Split Cells", fonts, |e| {
            let mut out = Outcome::Unchanged("nothing to split");
            if columns > 1 {
                out = e.split_cell(columns, fonts);
                if !matches!(out, Outcome::Changed(_)) {
                    return out;
                }
            }
            if rows > 1 {
                match e.split_cell_rows(rows, fonts) {
                    o @ Outcome::Changed(_) => out = o,
                    o if columns <= 1 => out = o,
                    _ => {}
                }
            }
            out
        })
    }

    /// The selected columns fitted to their contents.
    // shortcut: fitting to the window and fixed widths aren't built yet.
    pub(in crate::editing) fn auto_fit_columns(&mut self, to: AutoFit, fonts: &Fonts) -> Outcome {
        if to != AutoFit::Contents {
            return Outcome::unsupported("fitting columns to the window or fixed widths");
        }
        let Some(tg) = self.table_target() else { return Outcome::unavailable("not in a table") };
        self.joined("AutoFit", fonts, |e| {
            let mut out = Outcome::Unchanged("nothing to fit");
            for col in tg.cols.0..=tg.cols.1 {
                if let o @ Outcome::Changed(_) = e.auto_fit_column(tg.table, col, fonts) {
                    out = o;
                }
            }
            out
        })
    }

    pub(in crate::editing) fn set_cell_alignment(&mut self, a: CellAlignment, fonts: &Fonts) -> Outcome {
        let v = match a {
            CellAlignment::Top => "top",
            CellAlignment::Center => "center",
            CellAlignment::Bottom => "bottom",
        };
        self.table_format(&TableFormat::CellAlign(v.into()), fonts)
    }

    pub(in crate::editing) fn set_cell_borders(&mut self, b: CellBorders, fonts: &Fonts) -> Outcome {
        let edges = match b.edges {
            BorderEdges::All => Edges::All,
            BorderEdges::Outside => Edges::Outside,
            BorderEdges::Inside => Edges::Inside,
            BorderEdges::InsideHorizontal => Edges::InsideH,
            BorderEdges::InsideVertical => Edges::InsideV,
            BorderEdges::Top => Edges::Top,
            BorderEdges::Bottom => Edges::Bottom,
            BorderEdges::Left => Edges::Left,
            BorderEdges::Right => Edges::Right,
            BorderEdges::None => Edges::None,
        };
        let border = match b.border {
            Some(x) => {
                let style = match x.style {
                    BorderStyle::Single => "single",
                    BorderStyle::Double => "double",
                    BorderStyle::Dotted => "dotted",
                    BorderStyle::Dashed => "dashed",
                    BorderStyle::Thick => "thick",
                };
                let color = match x.color {
                    AutoOr::Auto => None,
                    AutoOr::Value(c) => Some(c.0),
                };
                Border { style: style.into(), width: x.width.0, space: 0.0, color }
            }
            None => Border { style: "single".into(), width: 0.5, space: 0.0, color: None },
        };
        self.table_format(&TableFormat::CellBorders(edges, border), fonts)
    }

    /// The selected cells' margins; those not given keep the first selected cell's.
    pub(in crate::editing) fn set_cell_margins(&mut self, m: CellMargins, fonts: &Fonts) -> Outcome {
        let Some(tg) = self.table_target() else { return Outcome::unavailable("not in a table") };
        let now = self
            .index
            .table(&self.doc, tg.table)
            .and_then(|t| {
                let g = Grid::of(t);
                let k = g.cell_at(tg.rows.0, tg.cols.0)?;
                Some(t.rows.get(tg.rows.0)?.cells.get(k)?.margins)
            })
            .unwrap_or([0.0, 5.4, 0.0, 5.4]);
        let pick = |v: Option<crate::api::value::Points>, d: f64| v.map_or(d, |v| v.0);
        let margins = [pick(m.top, now[0]), pick(m.left, now[1]), pick(m.bottom, now[2]), pick(m.right, now[3])];
        self.table_format(&TableFormat::CellMargins(margins), fonts)
    }

    pub(in crate::editing) fn set_header_row(&mut self, on: bool, fonts: &Fonts) -> Outcome {
        self.table_format(&TableFormat::HeaderRow(on), fonts)
    }

    pub(in crate::editing) fn set_row_can_break(&mut self, on: bool, fonts: &Fonts) -> Outcome {
        self.table_format(&TableFormat::RowCantSplit(!on), fonts)
    }

    pub(in crate::editing) fn set_row_height(&mut self, h: AutoOr<RowHeight>, fonts: &Fonts) -> Outcome {
        let h = match h {
            AutoOr::Auto => None,
            AutoOr::Value(h) => Some((h.height.0, if h.rule == HeightRule::Exact { "exact" } else { "atLeast" }.to_string())),
        };
        self.table_format(&TableFormat::RowHeight(h), fonts)
    }

    pub(in crate::editing) fn set_table_width(&mut self, w: TableWidth, fonts: &Fonts) -> Outcome {
        let w = match w {
            TableWidth::Auto => Width::Auto,
            TableWidth::Points { value } => Width::Dxa(value.0),
            TableWidth::Percent { value } => Width::Pct(value / 100.0),
        };
        self.table_format(&TableFormat::TableWidth(w), fonts)
    }

    pub(in crate::editing) fn set_table_alignment(&mut self, a: HorizontalAlignment, fonts: &Fonts) -> Outcome {
        let jc = match a {
            HorizontalAlignment::Left => Jc::Left,
            HorizontalAlignment::Center => Jc::Center,
            HorizontalAlignment::Right => Jc::Right,
        };
        self.table_format(&TableFormat::TableAlign(jc), fonts)
    }

    pub(in crate::editing) fn set_table_style(&mut self, id: StyleId, fonts: &Fonts) -> Outcome {
        if !self.doc.resolver.as_ref().is_some_and(|r| r.styles.by_id.contains_key(&id.0)) {
            return Outcome::invalid("no such style");
        }
        self.table_format(&TableFormat::TableStyle(id.0), fonts)
    }

    /// Which parts of the table style apply; those not given keep theirs (Word's default without `w:tblLook`: header
    /// row, banded rows and first column).
    pub(in crate::editing) fn set_table_style_options(&mut self, o: TableStyleChange, fonts: &Fonts) -> Outcome {
        let Some(tg) = self.table_target() else { return Outcome::unavailable("not in a table") };
        let Some(t) = self.index.table(&self.doc, tg.table) else { return Outcome::unavailable("not in a table") };
        let mut look = t.look.unwrap_or(crate::docx::props::Look {
            first_row: true,
            last_row: false,
            first_col: true,
            last_col: false,
            no_h_band: false,
            no_v_band: true,
        });
        let set = |slot: &mut bool, v: Option<bool>| {
            if let Some(v) = v {
                *slot = v;
            }
        };
        set(&mut look.first_row, o.header_row);
        set(&mut look.last_row, o.total_row);
        set(&mut look.first_col, o.first_column);
        set(&mut look.last_col, o.last_column);
        set(&mut look.no_h_band, o.banded_rows.map(|b| !b));
        set(&mut look.no_v_band, o.banded_columns.map(|b| !b));
        self.table_format(&TableFormat::TableLook(look), fonts)
    }
}
