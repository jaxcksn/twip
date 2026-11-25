//! Borders and Shading's values: each edge's line and the shading, of the selection's paragraph and text, or of
//! text in a style.

use super::Editor;
use crate::api::command::{Border, BorderStyle, ShadingPattern, ShadingTarget};
use crate::api::services::{BorderSetting, BordersShadingDialog, BordersTab};
use crate::api::value::{AutoOr, Color, NoneOr, Points};

/// An edge's line, or none.
pub(super) fn line(b: &Option<crate::docx::props::Border>) -> Option<Border> {
    let b = b.as_ref().filter(|b| b.visible())?;
    Some(Border {
        style: super::readback::api_border_style(b.style),
        width: Points(b.width),
        color: b.color.map_or(AutoOr::Auto, |c| AutoOr::Value(crate::api::drawing::ColourValue::rgb(Color(c)))),
        space: None,
        shadow: None,
    })
}

impl Editor {
    /// Borders and Shading's values for a table style's part (`region`, none for the whole table): its cells' edges,
    /// inside lines and shading.
    pub fn table_part_borders(&self, raw: &str, region: Option<&str>) -> Option<BordersShadingDialog> {
        let res = self.doc.resolver.as_ref()?;
        let mut next = (**res).clone();
        next.styles.set_xml(raw.to_string()).ok()?;
        let id = next.styles.by_id.values().find(|s| s.raw == raw)?.id.clone();
        let r = next.styles.table_region(Some(&id), &region.into_iter().collect::<Vec<_>>());
        // The whole table's edges are the table's; a part's, its cells'.
        let b = if region.is_none() { &r.tbl.borders } else { &r.tc.borders };
        let shd = r.tc.shd.clone();
        let mut v = self.borders_shading_values(None, false)?;
        (v.top, v.bottom, v.left, v.right) = (line(&b.top), line(&b.bottom), line(&b.left), line(&b.right));
        (v.between, v.inside_vertical) = (line(&b.inside_h), line(&b.inside_v));
        (v.diagonal_down, v.diagonal_up) = (line(&r.tc.borders.tl2br), line(&r.tc.borders.tr2bl));
        v.apply_to = ShadingTarget::Cell;
        let shd = shd.filter(|s| s.pattern != "nil");
        v.fill = shd.as_ref().map_or(r.tc.shading.flatten(), |s| s.fill).map_or(NoneOr::None, |c| NoneOr::Value(Color(c)));
        v.pattern = shd.as_ref().map_or(ShadingPattern::Clear, |s| ShadingPattern::of_val(&s.pattern));
        v.pattern_color = shd.as_ref().and_then(|s| s.color).map_or(AutoOr::Auto, |c| AutoOr::Value(Color(c)));
        v.setting = crate::services::dialogs::border_setting(&v, false);
        if let Some(l) =
            [&v.top, &v.left, &v.bottom, &v.right, &v.between, &v.inside_vertical, &v.diagonal_down, &v.diagonal_up].into_iter().flatten().next()
        {
            (v.style, v.color, v.width) = (l.style, super::readback::rgb_only(&l.color), l.width);
        }
        (v.page_top, v.page_bottom, v.page_left, v.page_right, v.page_setting) = (None, None, None, None, BorderSetting::None);
        Some(v)
    }

    /// Borders and Shading's values applied to the selection's cells (shown from the first) or to its table: their
    /// edges, each a cell's own else the table's, and their shading.
    pub fn table_borders_shading(&self, target: ShadingTarget) -> Option<BordersShadingDialog> {
        let mut v = self.borders_shading_values(None, false)?;
        let tg = self.table_target()?;
        let t = self.index.table(&self.doc, tg.table)?;
        let tb = &t.borders;
        let (edges, shd) = if target == ShadingTarget::Table {
            (tb.clone(), t.shd.clone())
        } else {
            let g = super::table::Grid::of(t);
            let r = tg.rows.0.min(t.rows.len().checked_sub(1)?);
            let k = g.rows.get(r)?.iter().position(|&(c, s, _)| tg.cols.0 >= c && tg.cols.0 < c + s.max(1))?;
            // A vertically merged cell: its first row's cell, its bottom edge the last row's.
            let (r, k) = g.merge_start(r, k);
            let cell = t.rows[r].cells.get(k)?;
            let (c, s, _) = g.rows[r][k];
            let mut end = r;
            while let Some(n) = g.cell_at(end + 1, c)
                && g.continues(end + 1, n)
            {
                end += 1;
            }
            let last = g.cell_at(end, c).and_then(|n| t.rows[end].cells.get(n)).unwrap_or(cell);
            let (last_row, last_col) = (end + 1 == t.rows.len(), c + s.max(1) >= g.cols);
            // Each row's own exceptions, else the table's, as layout draws them.
            let (tb, below) = (crate::layout::table::row_borders(t, r), crate::layout::table::row_borders(t, end));
            let own = |o: &Option<crate::docx::props::Border>, table: &Option<crate::docx::props::Border>| o.clone().or_else(|| table.clone());
            let shd = self.cell_shd(t, (r, k), cell);
            let [down, up] = cell.diagonals.as_deref().cloned().unwrap_or_default();
            (v.diagonal_down, v.diagonal_up) = (line(&down), line(&up));
            let edges = [
                own(&cell.top, &tb[if r == 0 { 0 } else { 4 }]),
                own(&cell.left, &tb[if c == 0 { 1 } else { 5 }]),
                own(&last.bottom, &below[if last_row { 2 } else { 4 }]),
                own(&cell.right, &tb[if last_col { 3 } else { 5 }]),
                tb[4].clone(),
                tb[5].clone(),
            ];
            (edges, shd)
        };
        (v.top, v.left, v.bottom, v.right, v.between, v.inside_vertical) =
            (line(&edges[0]), line(&edges[1]), line(&edges[2]), line(&edges[3]), line(&edges[4]), line(&edges[5]));
        let shd = shd.filter(|s| s.pattern != "nil");
        v.apply_to = target;
        v.fill = shd.as_ref().and_then(|s| s.fill).map_or(NoneOr::None, |c| NoneOr::Value(Color(c)));
        v.pattern = shd.as_ref().map_or(ShadingPattern::Clear, |s| ShadingPattern::of_val(&s.pattern));
        v.pattern_color = shd.as_ref().and_then(|s| s.color).map_or(AutoOr::Auto, |c| AutoOr::Value(Color(c)));
        v.setting = crate::services::dialogs::border_setting(&v, false);
        // The line new edges take: the first edge's, else the default pen.
        if let Some(l) =
            [&v.top, &v.left, &v.bottom, &v.right, &v.between, &v.inside_vertical, &v.diagonal_down, &v.diagonal_up].into_iter().flatten().next()
        {
            (v.style, v.color, v.width) = (l.style, super::readback::rgb_only(&l.color), l.width);
        } else {
            (v.style, v.color, v.width) = (BorderStyle::Single, AutoOr::Auto, Points(0.5));
        }
        Some(v)
    }

    /// A cell's shading as written: its table style's for the cell's region, under its own.
    pub(super) fn cell_shd(&self, t: &crate::docx::Table, (r, k): (usize, usize), cell: &crate::docx::Cell) -> Option<crate::docx::props::Shd> {
        let res = self.doc.resolver.as_ref();
        let flat = || {
            cell.shading.map(|f| crate::docx::props::Shd { pattern: "clear".into(), color: None, fill: Some(f), color_theme: None, fill_theme: None })
        };
        let Some(res) = res else { return flat() };
        let look = t.look.unwrap_or(crate::docx::props::Look {
            first_row: true,
            last_row: false,
            first_col: true,
            last_col: false,
            no_h_band: false,
            no_v_band: true,
        });
        let headers = t.rows.iter().enumerate().take_while(|(i, r)| r.header || *i == 0 && look.first_row).count();
        let ncols = t.rows.get(r).map_or(0, |row| row.cells.len());
        let mut tcp = res.cell_region(t.style_id.as_deref(), look, t.bands, (r, k), (t.rows.len(), ncols), headers).tc.clone();
        if let Some(raw) = cell.direct.0.as_deref() {
            tcp.merge(&self.cx().parse(raw, |_, n| crate::docx::props::TcProps::parse(Some(n), &res.styles.colours)).unwrap_or_default());
        }
        tcp.shd.or_else(flat)
    }

    /// Whether the selection is in a table.
    pub fn in_table(&self) -> bool {
        self.table_target().is_some()
    }

    /// Whether the selection is a block of cells: Borders and Shading then applies to them first.
    pub fn cells_selected(&self) -> bool {
        matches!(self.selection, super::Selection::Cells { .. })
    }

    /// Borders and Shading's values for the selection's paragraph and text (`style` none), or for text in a style's
    /// XML; `text` applies the shading to the text.
    pub fn borders_shading_values(&self, style: Option<&str>, text: bool) -> Option<BordersShadingDialog> {
        let (p, run) = match style {
            Some(raw) => {
                let (para, run, _) = self.style_formatting(raw)?;
                (para.style.clone(), run)
            }
            None => {
                let focus = self.selection.focus();
                match focus.and_then(|p| p.para_offset()) {
                    Some((id, _)) => {
                        let para = self.index.para(&self.doc, id)?;
                        (para.style.clone(), focus.and_then(|c| self.run_at(&c)).unwrap_or_else(|| para.mark.clone()))
                    }
                    // A block of cells: its first cell's first paragraph stands for the base.
                    None => {
                        let tg = self.table_target()?;
                        let cell = self.index.table(&self.doc, tg.table)?.rows.get(tg.rows.0)?.cells.first()?;
                        let para = cell.blocks.iter().find_map(|b| match b {
                            crate::docx::Block::Para(p) => Some(p),
                            crate::docx::Block::Table(_) => None,
                        })?;
                        (para.style.clone(), para.mark.clone())
                    }
                }
            }
        };
        // The shading as written: its pattern and colors; else the resolved fill.
        let shd = if text { run.shd.clone() } else { p.shd.clone() }.filter(|s| s.pattern != "nil");
        let fill = shd.as_ref().map_or(if text { run.shading } else { p.shading }, |s| s.fill);
        let pattern = shd.as_ref().map_or(ShadingPattern::Clear, |s| ShadingPattern::of_val(&s.pattern));
        let pattern_color = shd.as_ref().and_then(|s| s.color).map_or(AutoOr::Auto, |c| AutoOr::Value(Color(c)));
        // Text has one border, a box: every edge is its line.
        let boxed = text.then(|| line(&run.border));
        let edge = |b: &Option<crate::docx::props::Border>| boxed.clone().unwrap_or_else(|| line(b));
        let mut v = BordersShadingDialog {
            tab: BordersTab::Borders,
            setting: BorderSetting::None,
            style: BorderStyle::Single,
            color: AutoOr::Auto,
            width: Points(0.5),
            top: edge(&p.border_top),
            bottom: edge(&p.border_bottom),
            left: edge(&p.border_left),
            right: edge(&p.border_right),
            between: if text { None } else { line(&p.border_between) },
            apply_to: if text { ShadingTarget::Text } else { ShadingTarget::Paragraph },
            fill: fill.map_or(NoneOr::None, |c| NoneOr::Value(Color(c))),
            pattern,
            pattern_color,
            inside_vertical: None,
            diagonal_down: None,
            diagonal_up: None,
            page_setting: BorderSetting::None,
            page_top: None,
            page_bottom: None,
            page_left: None,
            page_right: None,
            page_offset_from: crate::api::command::PageBorderOffset::Page,
            page_display: crate::api::command::PageBorderDisplay::AllPages,
            page_apply_to: crate::api::command::PageBorderScope::ThisSection,
        };
        // The Page Border tab: the selection's section's (none for a style).
        if style.is_none()
            && let Some(b) = self.doc.sections.get(self.selection_section()).and_then(|s| s.page_borders.clone())
        {
            (v.page_top, v.page_bottom, v.page_left, v.page_right) = (line(&b.top), line(&b.bottom), line(&b.left), line(&b.right));
            v.page_offset_from =
                if b.offset_from_page { crate::api::command::PageBorderOffset::Page } else { crate::api::command::PageBorderOffset::Text };
            v.page_display = match b.display {
                crate::docx::PageBorderDisplay::FirstPage => crate::api::command::PageBorderDisplay::FirstPage,
                crate::docx::PageBorderDisplay::NotFirstPage => crate::api::command::PageBorderDisplay::NotFirstPage,
                crate::docx::PageBorderDisplay::AllPages => crate::api::command::PageBorderDisplay::AllPages,
            };
            v.page_setting = crate::services::dialogs::page_border_setting(&v, b.bottom.as_ref().is_some_and(|x| x.shadow));
        }
        let shadow = if text { run.border.as_ref().is_some_and(|b| b.shadow) } else { p.border_bottom.as_ref().is_some_and(|b| b.shadow) };
        v.setting = crate::services::dialogs::border_setting(&v, shadow);
        // The line new edges take: the first edge's.
        if let Some(b) = [&v.top, &v.left, &v.bottom, &v.right, &v.between, &v.page_top, &v.page_left].into_iter().flatten().next() {
            (v.style, v.color, v.width) = (b.style, super::readback::rgb_only(&b.color), b.width);
        }
        Some(v)
    }
}
