//! Table commands

use crate::api::command::{AutoFit, BorderEdges, CellBorders, CellMargins, Command, HorizontalSide, InsertColumns, SplitCells, TableStyleChange};
use crate::api::value::{AutoOr, CellAlignment, HeightRule, HorizontalAlignment, RowHeight, StyleId, TableWidth};
use crate::docx::props::{Border, Jc, Width};
use crate::font::Fonts;

use super::Outcome;
use crate::editing::Editor;
use crate::editing::table::Grid;
use crate::editing::table_format::{Edges, TableFormat};

impl Editor {
    pub(in crate::editing) fn insert_columns_by(&mut self, c: InsertColumns, fonts: &Fonts) -> Outcome {
        let right = c.side == HorizontalSide::Right;
        self.joined(crate::editing::revisions::record::INSERT_COLUMNS, fonts, |e| {
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

    pub(in crate::editing) fn split_cells(&mut self, s: SplitCells, fonts: &Fonts) -> Outcome {
        self.split_cells_into(s.columns as usize, s.rows.unwrap_or(1) as usize, fonts)
    }

    pub(in crate::editing) fn auto_fit_columns(&mut self, to: AutoFit, fonts: &Fonts) -> Outcome {
        let Some(tg) = self.table_target() else { return Outcome::unavailable("not in a table") };
        if to != AutoFit::Contents {
            let Some(t) = self.index.table(&self.doc, tg.table) else { return Outcome::unavailable("not in a table") };
            let mut grid = crate::editing::tables::full_grid(t, Grid::of(t).cols);
            if let Some(geo) = self.measured_pages(tg.table).first()
                && let Some(t) = self.index.table(&self.doc, tg.table)
            {
                crate::editing::tables::drawn_grid(geo, tg.table, t, &mut grid);
            }
            let fit = if to == AutoFit::Fixed {
                TableFormat::Fit { fixed: true, width: Width::Auto, grid }
            } else {
                // The columns keep their proportions across the text width.
                let avail = self.index.table_slot(tg.table).map_or(468.0, |s| self.text_width(s.container));
                let total: f64 = grid.iter().sum();
                let scale = if total > 0.0 { avail / total } else { 0.0 };
                let n = grid.len().max(1) as f64;
                let grid = grid.iter().map(|w| if scale > 0.0 { w * scale } else { avail / n }).collect();
                TableFormat::Fit { fixed: false, width: Width::Pct(1.0), grid }
            };
            return self.table_format_on(tg, &fit, fonts);
        }
        self.joined("AutoFit", fonts, |e| e.auto_fit_columns_to_contents(tg.table, tg.cols, fonts))
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
        let (edges, border) = cell_borders(b);
        self.table_format(&TableFormat::CellBorders(edges, border), fonts)
    }

    /// Sets edges `edges` of the selected cells to `line` (default: a ½ pt single automatic line), its colour an RGB or
    /// a theme colour with a tint or shade.
    pub fn set_cell_border_lines(&mut self, edges: BorderEdges, line: Option<&crate::editing::readback::BorderLine>, fonts: &Fonts) -> Outcome {
        let probe = Command::SetCellBorders(CellBorders { edges, border: None });
        if let Some(out) = self.refuse_like(&probe) {
            return out;
        }
        match self.border_lines(edges, line) {
            Ok((edges, border)) => self.table_format(&TableFormat::CellBorders(edges, border), fonts),
            Err(why) => Outcome::invalid(&why),
        }
    }

    /// `edges` and `line` as a table border edit (default: a ½ pt single automatic line).
    fn border_lines(&self, edges: BorderEdges, line: Option<&crate::editing::readback::BorderLine>) -> Result<(Edges, Border), String> {
        let (edges, mut border) = cell_borders(CellBorders { edges, border: None });
        if let Some(l) = line {
            let colour = crate::editing::readback::checked(self, l)?;
            border = Border {
                style: l.style.val().into(),
                width: l.width.0,
                space: l.space.0,
                shadow: l.shadow,
                color: colour.map(|c| c.rgb),
                color_theme: colour.and_then(|c| c.theme),
            };
        }
        Ok((edges, border))
    }

    /// Sets the selected rows' hidden end mark and their exceptions to the table's properties, the shading and
    /// border colours each an RGB or a theme colour with a tint or shade.
    pub fn set_row_exceptions(&mut self, hidden: Option<bool>, x: Option<&RowExceptionsEdit>, fonts: &Fonts) -> Outcome {
        let exceptions = match x.map(|x| self.row_ex(x)).transpose() {
            Ok(x) => x,
            Err(why) => return Outcome::invalid(&why),
        };
        if let Some(out) = self.refuse_like(&Command::SetRowProperties(Default::default())) {
            return out;
        }
        self.table_format(&TableFormat::RowProps { hidden, exceptions }, fonts)
    }

    fn row_ex(&self, x: &RowExceptionsEdit) -> Result<crate::editing::table_format::RowEx, String> {
        if let Some(i) = x.indent {
            if !(i.0.is_finite() && (-1584.0..=1584.0).contains(&i.0)) {
                return Err("a row's indent is -1584…1584 pt".into());
            }
            let fixed = self.table_target().and_then(|tg| self.index.table(&self.doc, tg.table)).is_some_and(|t| t.fixed);
            if fixed {
                return Err("a fixed-width table's rows take the table's indent".into());
            }
        }
        if let Some(s) = x.cell_spacing
            && !(s.0.is_finite() && (0.0..=1584.0).contains(&s.0))
        {
            return Err("a row's cell spacing is 0…1584 pt".into());
        }
        Ok(crate::editing::table_format::RowEx {
            reset: x.reset,
            jc: x.alignment.map(|a| match a {
                HorizontalAlignment::Center => Jc::Center,
                HorizontalAlignment::Right => Jc::Right,
                _ => Jc::Left,
            }),
            cell_spacing: x.cell_spacing.map(|s| s.0),
            ind: x.indent.map(|i| i.0),
            shading: x.shading.as_ref().map(|s| self.shading_of(s)).transpose()?,
            borders: x.borders.as_ref().map(|(edges, line)| self.border_lines(*edges, line.as_ref())).transpose()?,
        })
    }

    /// A shading value as the `w:shd` to write, its colours in this document.
    fn shading_of(&self, s: &crate::editing::readback::ShadingValue) -> Result<crate::editing::table_format::Shading, String> {
        let fill = match &s.fill {
            crate::api::value::NoneOr::Value(c) => Some(self.word_colour(c)?),
            crate::api::value::NoneOr::None => None,
        };
        let color = crate::editing::readback::word_colour_or_auto(self, &s.color)?;
        Ok(crate::editing::table_format::Shading { pattern: s.pattern.val(), color, fill })
    }

    pub(in crate::editing) fn set_default_table_style(&mut self, id: Option<String>, fonts: &Fonts) -> Outcome {
        if self.doc.settings.default_table_style == id {
            return Outcome::Unchanged("already so");
        }
        let mut tx = self.begin("Set as Default");
        if self.step(&mut tx, super::super::step::Step::DefaultTableStyle(id)).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("setting failed");
        }
        self.commit(tx, None, fonts)
    }

    pub(in crate::editing) fn set_row_properties(&mut self, p: crate::api::command::RowProperties, fonts: &Fonts) -> Outcome {
        let exceptions = p.exceptions.map(|x| super::super::table_format::RowEx {
            reset: x.reset.unwrap_or(false),
            jc: x.alignment.map(|a| match a {
                HorizontalAlignment::Center => Jc::Center,
                HorizontalAlignment::Right => Jc::Right,
                _ => Jc::Left,
            }),
            cell_spacing: x.cell_spacing.map(|s| s.0),
            ind: None,
            shading: x.shading.map(|c| crate::editing::table_format::Shading::fill(Some(crate::editing::theme::WordColour::rgb(c.0)))),
            borders: x.borders.map(cell_borders),
        });
        self.table_format(&TableFormat::RowProps { hidden: p.hidden, exceptions }, fonts)
    }
}

/// `setCellBorders`'s edges and line.
pub(in crate::editing) fn cell_borders(b: CellBorders) -> (Edges, Border) {
    {
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
            BorderEdges::DiagonalDown => Edges::DiagonalDown,
            BorderEdges::DiagonalUp => Edges::DiagonalUp,
            BorderEdges::None => Edges::None,
        };
        let border = match b.border {
            Some(x) => {
                let style = x.style.val();
                let color = match x.color {
                    AutoOr::Auto => None,
                    AutoOr::Value(c) => Some(c.0),
                };
                Border { style: style.into(), width: x.width.0, space: 0.0, shadow: false, color, color_theme: None }
            }
            None => Border { style: "single".into(), width: 0.5, space: 0.0, shadow: false, color: None, color_theme: None },
        };
        (edges, border)
    }
}

impl Editor {
    /// The selection's first cell's margins: top, left, bottom, right.
    pub fn cell_margins_here(&self) -> Option<[f64; 4]> {
        let tg = self.table_target()?;
        let t = self.index.table(&self.doc, tg.table)?;
        let k = Grid::of(t).cell_at(tg.rows.0, tg.cols.0)?;
        Some(t.rows.get(tg.rows.0)?.cells.get(k)?.margins)
    }

    pub(in crate::editing) fn set_cell_margins(&mut self, m: CellMargins, fonts: &Fonts) -> Outcome {
        if self.table_target().is_none() {
            return Outcome::unavailable("not in a table");
        }
        let now = self.cell_margins_here().unwrap_or([0.0, 5.4, 0.0, 5.4]);
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
        match self.doc.resolver.as_ref().and_then(|r| r.styles.by_id.get(&id.0)) {
            None => return Outcome::invalid("no such style"),
            Some(s) if s.kind != crate::docx::styles::StyleKind::Table => return Outcome::invalid("not a table style"),
            Some(_) => {}
        }
        self.table_format(&TableFormat::TableStyle(id.0), fonts)
    }

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

/// Row exceptions to the table's properties to set ([`Editor::set_row_exceptions`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RowExceptionsEdit {
    /// Removes the rows' exceptions first.
    pub reset: bool,
    /// The rows' alignment.
    pub alignment: Option<HorizontalAlignment>,
    /// The space between the rows' cells, 0 to 1584 pt.
    pub cell_spacing: Option<crate::api::value::Points>,
    /// The rows' indent from the table's left edge, -1584 to 1584 pt.
    pub indent: Option<crate::api::value::Points>,
    /// The rows' shading: fill, pattern and pattern colour.
    pub shading: Option<crate::editing::readback::ShadingValue>,
    /// The table borders on the rows' edges and their line (`None`: a ½ pt single automatic line).
    pub borders: Option<(BorderEdges, Option<crate::editing::readback::BorderLine>)>,
}
